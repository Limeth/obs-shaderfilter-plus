use std::borrow::Cow;
use std::path::PathBuf;
use std::ffi::CString;
use obs_wrapper::{
    graphics::*,
    source::*,
};
use downcast::{impl_downcast, Downcast};
use regex::Regex;
use crate::*;

mod effect_param;
mod loaded_value;

pub use effect_param::*;
pub use loaded_value::*;

/// An object representing a binding of setting-properties to graphics uniforms.
pub trait BindableProperty: Downcast {
    fn add_properties(&self, properties: &mut Properties);
    fn reload_settings(&mut self, settings: &mut SettingsContext);
    fn prepare_values(&mut self);
    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext);
    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext);
    fn enable_and_drop(self, graphics_context: &GraphicsContext);
}
impl_downcast!(BindableProperty);

#[derive(Default)]
pub struct EffectParamsCustom {
    // Custom effect params sorted by their order in source
    pub params: Vec<Box<dyn BindableProperty>>,
}

impl EffectParamsCustom {
    pub fn from<'a>(
        mut params: HashMap<String, Indexed<GraphicsContextDependentEnabled<'a, GraphicsEffectParam>>>,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        use ShaderParamTypeKind::*;

        let mut bound_params: Vec<Indexed<Box<dyn BindableProperty>>> = Vec::new();

        let result: Result<(), Cow<'static, str>> = try {
            {
                let pattern_builtin_texture_fft = Regex::new(r"^builtin_texture_fft_(?P<field>\w+)$").unwrap();
                let pattern_field_previous = Regex::new(r"^.*_previous$").unwrap();
                let param_names = params.keys().cloned().collect::<Vec<_>>();

                for param_name in &param_names {
                    let captures = if let Some(captures) = pattern_builtin_texture_fft.captures(&param_name) {
                        captures
                    } else {
                        continue;
                    };
                    let field_name = captures.name("field").unwrap().as_str();

                    if pattern_field_previous.is_match(&field_name) {
                        continue;
                    }

                    let (param_index, param) = params.remove(param_name).unwrap().into_tuple();
                    let param_previous = params.remove(&format!("{}_previous", param_name))
                        .map(|indexed| indexed.into_inner());

                    if param.param_type() != Texture {
                        throw!(format!("Builtin field `{}` must be of type `{}`", field_name, "texture2d"));
                    }

                    if let Some(ref param_previous) = param_previous.as_ref() {
                        if param_previous.param_type() != Texture {
                            throw!(format!("Builtin field `{}` must be of type `{}`", field_name, "texture2d"));
                        }
                    }

                    bound_params.push(
                        Indexed {
                            index: param_index,
                            inner: Box::new(EffectParamCustomFFT::new(
                                param.downcast().unwrap(),
                                param_previous.map(|param_previous| param_previous.downcast().unwrap()),
                                field_name,
                                settings,
                                preprocess_result,
                            )?),
                        },
                    );
                }
            }
        };

        result.map_err(|err| {
            Cow::Owned(format!("An error occurred while binding effect uniform variable: {}", err))
        })?;

        let pattern_param_builtin = Regex::new(r"^builtin_").unwrap();

        for (_index, param) in params {
            let param_name = param.name().to_string();

            // Ensure custom uniform properties do not conflict with builtin properties.
            if pattern_param_builtin.is_match(&param_name) {
                throw!(format!("Unrecognized `builtin_` uniform variable type: {}", &param_name));
            }

            Self::add_param(&mut bound_params, param, &param_name, settings, preprocess_result)
                .map_err(|err| {
                    Cow::Owned(format!("An error occurred while binding effect uniform variable `{}`: {}", param_name, err))
                })?;
        }

        // Ensure the properties are stored in the order they were declared
        bound_params.sort_unstable();

        Ok(Self {
            params: bound_params.into_iter()
                .map(|indexed| indexed.into_inner())
                .collect(),
        })
    }

    pub fn add_param<'a>(
        bound_params: &mut Vec<Indexed<Box<dyn BindableProperty>>>,
        param: Indexed<GraphicsContextDependentEnabled<'a, GraphicsEffectParam>>,
        param_name: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<(), Cow<'static, str>> {
        use ShaderParamTypeKind::*;

        let bindable: Indexed<Box<dyn BindableProperty>> = match param.param_type() {
            Unknown => throw!("Cannot add an effect param of unknown type. Make sure to use HLSL type names for uniform variables."),
            Bool  => param.map(|param| {
                EffectParamCustomBool::new(param.downcast().unwrap(), &param_name, settings, preprocess_result)
                    .map(|param| Box::new(param) as Box<dyn BindableProperty>)
            }).transpose()?,
            Float  => param.map(|param| {
                EffectParamCustomFloat::new(param.downcast().unwrap(), &param_name, settings, preprocess_result)
                    .map(|param| Box::new(param) as Box<dyn BindableProperty>)
            }).transpose()?,
            Int  => param.map(|param| {
                EffectParamCustomInt::new(param.downcast().unwrap(), &param_name, settings, preprocess_result)
                    .map(|param| Box::new(param) as Box<dyn BindableProperty>)
            }).transpose()?,
            Vec4  => param.map(|param| {
                EffectParamCustomColor::new(param.downcast().unwrap(), &param_name, settings, preprocess_result)
                    .map(|param| Box::new(param) as Box<dyn BindableProperty>)
            }).transpose()?,
            Vec2 | Vec3 | IVec2 | IVec3 | IVec4 | Mat4 => {
                throw!("Multi-component types as effect params are not yet supported.");
            },
            String => throw!("Strings as effect params are not yet supported."),
            Texture => throw!("Textures as effect params are not yet supported."),
        };

        bound_params.push(bindable);

        Ok(())
    }

    pub fn reload_settings(&mut self, settings: &mut SettingsContext) {
        self.params.iter_mut().for_each(|param| param.reload_settings(settings));
    }

    pub fn prepare_values(&mut self) {
        self.params.iter_mut().for_each(|param| param.prepare_values());
    }

    pub fn stage_values(&mut self, graphics_context: &GraphicsContext) {
        self.params.iter_mut().for_each(|param| param.stage_value(graphics_context));
    }

    pub fn assign_values(&mut self, graphics_context: &FilterContext) {
        self.params.iter_mut().for_each(|param| param.assign_value(graphics_context));
    }

    pub fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        #[allow(unused_assignments)]
        self.params.into_iter().for_each(|mut param| {
            param = match param.downcast::<EffectParamCustomBool>() {
                Ok(param) => return param.enable_and_drop(graphics_context),
                Err(param) => param,
            };
            param = match param.downcast::<EffectParamCustomFloat>() {
                Ok(param) => return param.enable_and_drop(graphics_context),
                Err(param) => param,
            };
            param = match param.downcast::<EffectParamCustomInt>() {
                Ok(param) => return param.enable_and_drop(graphics_context),
                Err(param) => param,
            };
            param = match param.downcast::<EffectParamCustomColor>() {
                Ok(param) => return param.enable_and_drop(graphics_context),
                Err(param) => param,
            };
            param = match param.downcast::<EffectParamCustomFFT>() {
                Ok(param) => return param.enable_and_drop(graphics_context),
                Err(param) => param,
            };
            panic!("No registered downcast to `enable_and_drop` a `Box<dyn BindableProperty>`. This is an implementation error.");
        });
    }

    pub fn add_properties(&self, properties: &mut Properties) {
        self.params.iter().for_each(|param| param.add_properties(properties));
    }
}

pub struct EffectParams {
    pub frame: EffectParamInt,
    pub framerate: EffectParamFloat,
    pub elapsed_time: EffectParamFloat,
    pub elapsed_time_previous: EffectParamFloat,
    pub elapsed_time_since_shown: EffectParamFloat,
    pub elapsed_time_since_shown_previous: EffectParamFloat,
    pub elapsed_time_since_enabled: EffectParamFloat,
    pub elapsed_time_since_enabled_previous: EffectParamFloat,
    pub uv_size: EffectParamIVec2,
    pub custom: EffectParamsCustom,
}

impl EffectParams {
    pub fn reload_settings(&mut self, settings: &mut SettingsContext) {
        self.custom.reload_settings(settings);
    }

    pub fn stage_values(&mut self, graphics_context: &GraphicsContext) {
        self.frame.stage_value(graphics_context);
        self.framerate.stage_value(graphics_context);
        self.elapsed_time.stage_value(graphics_context);
        self.elapsed_time_previous.stage_value(graphics_context);
        self.elapsed_time_since_shown.stage_value(graphics_context);
        self.elapsed_time_since_shown_previous.stage_value(graphics_context);
        self.elapsed_time_since_enabled.stage_value(graphics_context);
        self.elapsed_time_since_enabled_previous.stage_value(graphics_context);
        self.uv_size.stage_value(graphics_context);
        self.custom.stage_values(graphics_context);
    }

    pub fn assign_values(&mut self, graphics_context: &FilterContext) {
        self.frame.assign_value(graphics_context);
        self.framerate.assign_value(graphics_context);
        self.elapsed_time.assign_value(graphics_context);
        self.elapsed_time_previous.assign_value(graphics_context);
        self.elapsed_time_since_shown.assign_value(graphics_context);
        self.elapsed_time_since_shown_previous.assign_value(graphics_context);
        self.elapsed_time_since_enabled.assign_value(graphics_context);
        self.elapsed_time_since_enabled_previous.assign_value(graphics_context);
        self.uv_size.assign_value(graphics_context);
        self.custom.assign_values(graphics_context);
    }

    pub fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.frame.enable_and_drop(graphics_context);
        self.framerate.enable_and_drop(graphics_context);
        self.elapsed_time.enable_and_drop(graphics_context);
        self.elapsed_time_previous.enable_and_drop(graphics_context);
        self.elapsed_time_since_shown.enable_and_drop(graphics_context);
        self.elapsed_time_since_shown_previous.enable_and_drop(graphics_context);
        self.elapsed_time_since_enabled.enable_and_drop(graphics_context);
        self.elapsed_time_since_enabled_previous.enable_and_drop(graphics_context);
        self.uv_size.enable_and_drop(graphics_context);
        self.custom.enable_and_drop(graphics_context);
    }

    pub fn add_properties(&self, properties: &mut Properties) {
        self.custom.add_properties(properties);
    }
}

pub struct PreparedEffect {
    pub effect: GraphicsContextDependentDisabled<GraphicsEffect>,
    pub shader_source: String,
    pub params: EffectParams,
}

impl PreparedEffect {
    pub fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.effect.enable(graphics_context);
        self.params.enable_and_drop(graphics_context);
    }

    pub fn add_properties(&self, properties: &mut Properties) {
        self.params.add_properties(properties);
    }

    pub fn create_effect<'a>(shader_path: &PathBuf, shader_source: &str, graphics_context: &'a GraphicsContext) -> Result<(GraphicsContextDependentEnabled<'a, GraphicsEffect>, PreprocessResult), Cow<'static, str>> {
        const EFFECT_SOURCE_TEMPLATE: &'static str = include_str!("../effect_template.effect");

        let (preprocess_result, effect_source) = {
            let pattern = Regex::new(r"(?P<shader>__SHADER__)").unwrap();
            let effect_source = pattern.replace_all(EFFECT_SOURCE_TEMPLATE, shader_source);

            let (preprocess_result, effect_source) = preprocess(&effect_source);

            (preprocess_result, effect_source.into_owned())
        };

        let shader_path_c = CString::new(
            shader_path.to_str().ok_or_else(|| {
                "Specified shader path is not a valid UTF-8 string."
            })?
        ).map_err(|_| "Shader path cannot be converted to a C string.")?;
        let effect_source_c = CString::new(effect_source.clone())
            .map_err(|_| "Shader contents cannot be converted to a C string.")?;

        let effect = {
            let capture = LogCaptureHandler::new(LogLevel::Error).unwrap();
            let result = GraphicsEffect::from_effect_string(
                effect_source_c.as_c_str(),
                shader_path_c.as_c_str(),
                &graphics_context,
            );

            result.map_err(|err| {
                if let Some(err) = err {
                    Cow::Owned(format!("Could not create the effect due to the following error: {}", err))
                } else {
                    Cow::Owned(format!("Could not create the effect due to the following error:\n{}", capture.to_string()))
                }
            })
        }?;

        Ok((effect, preprocess_result))
    }
}
