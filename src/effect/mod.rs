use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::borrow::Cow;
use std::time::{Instant, Duration};
use std::path::PathBuf;
use std::fs::File;
use std::ffi::{CStr, CString};
use std::io::Read;
use ammolite_math::*;
use obs_sys::{
    MAX_AUDIO_MIXES,
    MAX_AUDIO_CHANNELS,
};
use obs_wrapper::{
    graphics::*,
    obs_register_module,
    prelude::*,
    source::*,
    context::*,
    audio::*,
};
use smallvec::{SmallVec, smallvec};
use regex::Regex;
use paste::item;
use crate::preprocessor::*;
use crate::*;

mod effect_param;
mod loaded_value;

pub use effect_param::*;
pub use loaded_value::*;

// TODO: Consider implementing on types
// /// An object representing a binding of setting-properties to graphics uniforms.
// pub trait BindableProperty {
//     fn add_properties(&self, properties: &mut Properties);
//     fn prepare_values(&mut self);
//     fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext);
//     fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext);
//     fn enable_and_drop(self, graphics_context: &GraphicsContext);
// }

pub struct EffectParamCustomFFT {
    pub effect_param: EffectParamTexture,
    pub effect_param_previous: Option<EffectParamTexture>,
    pub audio_fft: Arc<GlobalStateAudioFFT>,
    pub property_mix: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorI32>,
    pub property_channel: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorI32>,
    pub property_dampening_factor_attack: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorF64>,
    pub property_dampening_factor_release: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorF64>,
}

impl EffectParamCustomFFT {
    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<ShaderParamTypeTexture>>,
        param_previous: Option<GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<ShaderParamTypeTexture>>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property_mix = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: false,
                default_value: 1,
                default_descriptor_specialization: PropertyDescriptorSpecializationI32 {
                    min: 1,
                    max: MAX_AUDIO_MIXES as i32,
                    step: 1,
                    slider: false,
                },
            },
            identifier,
            Some("mix"),
            preprocess_result,
            settings,
        )?;
        let property_channel = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: false,
                default_value: 1,
                default_descriptor_specialization: PropertyDescriptorSpecializationI32 {
                    min: 1,
                    max: 2, // FIXME: Causes crashes when `MAX_AUDIO_CHANNELS as i32` is used, supposedly fixed in next OBS release
                    step: 1,
                    slider: false,
                },
            },
            identifier,
            Some("channel"),
            preprocess_result,
            settings,
        )?;
        let property_dampening_factor_attack = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: 0.0,
                default_descriptor_specialization: PropertyDescriptorSpecializationF64 {
                    min: 0.0,
                    max: 100.0,
                    step: 0.01,
                    slider: true,
                },
            },
            identifier,
            Some("dampening_factor_attack"),
            preprocess_result,
            settings,
        )?;
        let property_dampening_factor_release = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: 0.0,
                default_descriptor_specialization: PropertyDescriptorSpecializationF64 {
                    min: 0.0,
                    max: 100.0,
                    step: 0.01,
                    slider: true,
                },
            },
            identifier,
            Some("dampening_factor_release"),
            preprocess_result,
            settings,
        )?;

        let audio_fft_descriptor = GlobalStateAudioFFTDescriptor::new(
            property_mix.get_value() as usize - 1,
            property_channel.get_value() as usize - 1,
            property_dampening_factor_attack.get_value() / 100.0,
            property_dampening_factor_release.get_value() / 100.0,
            // TODO: Make customizable, but provide a sane default value
            WindowFunction::Hanning,
        );

        Ok(Self {
            effect_param: EffectParam::new(param.disable()),
            effect_param_previous: param_previous.map(|param_previous| EffectParam::new(param_previous.disable())),
            audio_fft: GLOBAL_STATE.request_audio_fft(&audio_fft_descriptor),
            property_mix,
            property_channel,
            property_dampening_factor_attack,
            property_dampening_factor_release,
        })
    }

    fn add_properties(&self, properties: &mut Properties) {
        self.property_mix.add_properties(properties);
        self.property_channel.add_properties(properties);
        self.property_dampening_factor_attack.add_properties(properties);
        self.property_dampening_factor_release.add_properties(properties);
    }

    fn prepare_values(&mut self) {
        let fft_result = if let Some(result) = self.audio_fft.retrieve_result() {
            result
        } else {
            return;
        };
        let frequency_spectrum = &fft_result.frequency_spectrum;
        let texture_data = unsafe {
            std::slice::from_raw_parts::<u8>(
                frequency_spectrum.as_ptr() as *const _,
                frequency_spectrum.len() * std::mem::size_of::<f32>(),
            )
        }.iter().copied().collect::<Vec<_>>();
        let texture_fft = TextureDescriptor {
            dimensions: [frequency_spectrum.len(), 1],
            color_format: ColorFormatKind::R32F,
            levels: smallvec![texture_data],
            flags: 0,
        };

        self.effect_param.prepare_value(texture_fft);
    }

    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        if let Some(effect_param_previous) = self.effect_param_previous.as_mut() {
            if let Some(previous_texture_fft) = self.effect_param.take_staged_value() {
                effect_param_previous.stage_value_custom(previous_texture_fft, graphics_context);
            }
        }

        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        if let Some(effect_param_previous) = self.effect_param_previous.as_mut() {
            effect_param_previous.assign_value_if_staged(graphics_context);
        }
        self.effect_param.assign_value_if_staged(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        if let Some(effect_param_previous) = self.effect_param_previous {
            effect_param_previous.enable_and_drop(graphics_context);
        }
        self.effect_param.enable_and_drop(graphics_context);
    }
}

#[derive(Default)]
pub struct EffectParamsCustom {
    pub params_bool: Vec<EffectParamCustomBool>,
    pub params_float: Vec<EffectParamCustomFloat>,
    pub params_int: Vec<EffectParamCustomInt>,
    pub params_vec4: Vec<EffectParamCustomColor>,
    pub params_fft: Vec<EffectParamCustomFFT>,
    // TODO: Textures
}

impl EffectParamsCustom {
    pub fn prepare_values(&mut self) {
        self.params_bool.iter_mut().for_each(|param| param.prepare_values());
        self.params_float.iter_mut().for_each(|param| param.prepare_values());
        self.params_int.iter_mut().for_each(|param| param.prepare_values());
        self.params_vec4.iter_mut().for_each(|param| param.prepare_values());
        self.params_fft.iter_mut().for_each(|param| param.prepare_values());
    }

    pub fn stage_values(&mut self, graphics_context: &GraphicsContext) {
        self.params_bool.iter_mut().for_each(|param| param.stage_value(graphics_context));
        self.params_float.iter_mut().for_each(|param| param.stage_value(graphics_context));
        self.params_int.iter_mut().for_each(|param| param.stage_value(graphics_context));
        self.params_vec4.iter_mut().for_each(|param| param.stage_value(graphics_context));
        self.params_fft.iter_mut().for_each(|param| param.stage_value(graphics_context));
    }

    pub fn assign_values(&mut self, graphics_context: &FilterContext) {
        self.params_bool.iter_mut().for_each(|param| param.assign_value(graphics_context));
        self.params_float.iter_mut().for_each(|param| param.assign_value(graphics_context));
        self.params_int.iter_mut().for_each(|param| param.assign_value(graphics_context));
        self.params_vec4.iter_mut().for_each(|param| param.assign_value(graphics_context));
        self.params_fft.iter_mut().for_each(|param| param.assign_value(graphics_context));
    }

    pub fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.params_bool.into_iter().for_each(|param| param.enable_and_drop(graphics_context));
        self.params_float.into_iter().for_each(|param| param.enable_and_drop(graphics_context));
        self.params_int.into_iter().for_each(|param| param.enable_and_drop(graphics_context));
        self.params_vec4.into_iter().for_each(|param| param.enable_and_drop(graphics_context));
        self.params_fft.into_iter().for_each(|param| param.enable_and_drop(graphics_context));
    }

    pub fn add_properties(&self, properties: &mut Properties) {
        self.params_bool.iter().for_each(|param| param.add_properties(properties));
        self.params_float.iter().for_each(|param| param.add_properties(properties));
        self.params_int.iter().for_each(|param| param.add_properties(properties));
        self.params_vec4.iter().for_each(|param| param.add_properties(properties));
        self.params_fft.iter().for_each(|param| param.add_properties(properties));
    }

    pub fn add_param<'a>(
        &mut self,
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParam>,
        param_name: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<(), Cow<'static, str>> {
        use ShaderParamTypeKind::*;

        Ok(match param.param_type() {
            Unknown => throw!("Cannot add an effect param of unknown type. Make sure to use HLSL type names for uniform variables."),
            Bool  => self.params_bool.push(EffectParamCustomBool::new(param.downcast().unwrap(), &param_name, settings, preprocess_result)?),
            Float => self.params_float.push(EffectParamCustomFloat::new(param.downcast().unwrap(), &param_name, settings, preprocess_result)?),
            Int   => self.params_int.push(EffectParamCustomInt::new(param.downcast().unwrap(), &param_name, settings, preprocess_result)?),
            Vec4  => self.params_vec4.push(EffectParamCustomColor::new(param.downcast().unwrap(), &param_name, settings, preprocess_result)?),
            Vec2 | Vec3 | IVec2 | IVec3 | IVec4 | Mat4 => {
                throw!("Multi-component types as effect params are not yet supported.");
            },
            String => throw!("Strings as effect params are not yet supported."),
            Texture => throw!("Textures as effect params are not yet supported."),
        })
    }

    pub fn add_params<'a>(
        &mut self,
        mut params: HashMap<String, GraphicsContextDependentEnabled<'a, GraphicsEffectParam>>,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<(), Cow<'static, str>> {
        use ShaderParamTypeKind::*;

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

                    let param = params.remove(param_name).unwrap();
                    let param_previous = params.remove(&format!("{}_previous", param_name));

                    if param.param_type() != Texture {
                        throw!(format!("Builtin field `{}` must be of type `{}`", field_name, "texture2d"));
                    }

                    if let Some(ref param_previous) = param_previous.as_ref() {
                        if param_previous.param_type() != Texture {
                            throw!(format!("Builtin field `{}` must be of type `{}`", field_name, "texture2d"));
                        }
                    }

                    self.params_fft.push(EffectParamCustomFFT::new(
                            param.downcast().unwrap(),
                            param_previous.map(|param_previous| param_previous.downcast().unwrap()),
                            field_name,
                            settings,
                            preprocess_result,
                    )?);
                }
            }
        };

        result?;

        for (param_name, param) in params {
            self.add_param(param, &param_name, settings, preprocess_result)
                .map_err(|err| {
                    Cow::Owned(format!("An error occurred while binding effect uniform variable `{}`: {}", param_name, err))
                })?;
        }

        Ok(())
    }
}

pub struct EffectParams {
    pub elapsed_time: EffectParamFloat,
    pub uv_size: EffectParamIVec2,
    pub custom: EffectParamsCustom,
}

impl EffectParams {
    pub fn stage_values(&mut self, graphics_context: &GraphicsContext) {
        self.elapsed_time.stage_value(graphics_context);
        self.uv_size.stage_value(graphics_context);
        self.custom.stage_values(graphics_context);
    }

    pub fn assign_values(&mut self, graphics_context: &FilterContext) {
        self.elapsed_time.assign_value(graphics_context);
        self.uv_size.assign_value(graphics_context);
        self.custom.assign_values(graphics_context);
    }

    pub fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.elapsed_time.enable_and_drop(graphics_context);
        self.uv_size.enable_and_drop(graphics_context);
        self.custom.enable_and_drop(graphics_context);
    }

    pub fn add_properties(&self, properties: &mut Properties) {
        self.custom.add_properties(properties);
    }
}

pub struct PreparedEffect {
    pub effect: GraphicsContextDependentDisabled<GraphicsEffect>,
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
}
