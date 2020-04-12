#![feature(try_blocks)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::borrow::Cow;
use std::time::{Instant, Duration};
use std::path::PathBuf;
use std::fs::File;
use std::ffi::{CStr, CString};
use std::io::Read;
use ammolite_math::*;
use obs_wrapper::{graphics::*, obs_register_module, prelude::*, source::*};
use regex::Regex;

macro_rules! throw {
    ($e:expr) => {{
        Err($e)?;
        unreachable!()
    }}
}

// use crossbeam_channel::{unbounded, Receiver, Sender};

struct EffectParam<T: ShaderParamType> where T::RustType: Default + Clone {
    param: GraphicsEffectParamTyped<T>,
    value: T::RustType,
}

impl<T: ShaderParamType> EffectParam<T> where T::RustType: Default + Clone {
    fn new(param: GraphicsEffectParamTyped<T>) -> Self {
        Self {
            param,
            value: Default::default(),
        }
    }

    fn prepare_value(&mut self, new_value: T::RustType) {
        self.value = new_value;
    }

    fn assign_value(&mut self) {
        self.param.set_param_value(self.value.clone());
    }
}

trait EffectParamCustom {
    type ShaderParamType: ShaderParamType;
    type PropertyDescriptorSpecialization: PropertyDescriptorSpecialization;

    fn new(param: GraphicsEffectParamTyped<Self::ShaderParamType>, settings: &mut SettingsContext) -> Self;
    fn assign_value(&mut self);
}

struct EffectParamCustomBool {
    effect_param: EffectParam<ShaderParamTypeBool>,
    property_descriptor: PropertyDescriptor<PropertyDescriptorSpecializationBool>,
}

impl EffectParamCustom for EffectParamCustomBool {
    type ShaderParamType = ShaderParamTypeBool;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationBool;

    fn new(param: GraphicsEffectParamTyped<Self::ShaderParamType>, settings: &mut SettingsContext) -> Self {
        let mut result = Self {
            property_descriptor: PropertyDescriptor {
                name: CString::new(param.inner.name()).unwrap(),
                description: CString::new(param.inner.name()).unwrap(),
                specialization: Self::PropertyDescriptorSpecialization {},
            },
            effect_param: EffectParam::new(param),
        };
        let default_value = *result.effect_param.param.get_param_value_default();
        let loaded_value = settings.get_property_value(&result.property_descriptor, &default_value);

        result.effect_param.prepare_value(loaded_value);

        result
    }

    fn assign_value(&mut self) {
        self.effect_param.assign_value()
    }
}

struct EffectParamCustomInt {
    effect_param: EffectParam<ShaderParamTypeInt>,
    property_descriptor: PropertyDescriptor<PropertyDescriptorSpecializationI32>,
}

impl EffectParamCustom for EffectParamCustomInt {
    type ShaderParamType = ShaderParamTypeInt;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationI32;

    fn new(param: GraphicsEffectParamTyped<Self::ShaderParamType>, settings: &mut SettingsContext) -> Self {
        let mut result = Self {
            property_descriptor: PropertyDescriptor {
                name: CString::new(param.inner.name()).unwrap(),
                description: CString::new(param.inner.name()).unwrap(),
                specialization: Self::PropertyDescriptorSpecialization {
                    min: std::i32::MIN,
                    max: std::i32::MAX,
                    step: 1,
                    slider: false,
                },
            },
            effect_param: EffectParam::new(param),
        };
        let default_value = *result.effect_param.param.get_param_value_default();
        let loaded_value = settings.get_property_value(&result.property_descriptor, &default_value);

        result.effect_param.prepare_value(loaded_value);

        result
    }

    fn assign_value(&mut self) {
        self.effect_param.assign_value()
    }
}

struct EffectParamCustomFloat {
    effect_param: EffectParam<ShaderParamTypeFloat>,
    property_descriptor: PropertyDescriptor<PropertyDescriptorSpecializationF64>,
}

impl EffectParamCustom for EffectParamCustomFloat {
    type ShaderParamType = ShaderParamTypeFloat;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationF64;

    fn new(param: GraphicsEffectParamTyped<Self::ShaderParamType>, settings: &mut SettingsContext) -> Self {
        let mut result = Self {
            property_descriptor: PropertyDescriptor {
                name: CString::new(param.inner.name()).unwrap(),
                description: CString::new(param.inner.name()).unwrap(),
                specialization: Self::PropertyDescriptorSpecialization {
                    min: std::f64::MIN,
                    max: std::f64::MAX,
                    step: 0.1,
                    slider: false,
                },
            },
            effect_param: EffectParam::new(param),
        };
        let default_value = (*result.effect_param.param.get_param_value_default()) as f64;
        let loaded_value = settings.get_property_value(&result.property_descriptor, &default_value);

        result.effect_param.prepare_value(loaded_value as f32);

        result
    }

    fn assign_value(&mut self) {
        self.effect_param.assign_value()
    }
}

#[derive(Default)]
struct EffectParamsCustom {
    params_bool: Vec<EffectParamCustomBool>,
    params_float: Vec<EffectParamCustomFloat>,
    params_int: Vec<EffectParamCustomInt>,
    // TODO: Textures
}

impl EffectParamsCustom {
    fn assign_values(&mut self) {
        self.params_bool.iter_mut().for_each(EffectParamCustom::assign_value);
        self.params_float.iter_mut().for_each(EffectParamCustom::assign_value);
        self.params_int.iter_mut().for_each(EffectParamCustom::assign_value);
    }

    fn add_properties(&self, properties: &mut Properties) {
        self.params_bool.iter().for_each(|param| properties.add_property(&param.property_descriptor));
        self.params_float.iter().for_each(|param| properties.add_property(&param.property_descriptor));
        self.params_int.iter().for_each(|param| properties.add_property(&param.property_descriptor));
    }
}

impl EffectParamsCustom {
    fn add_param(&mut self, param: GraphicsEffectParam, settings: &mut SettingsContext) -> Result<(), Cow<'static, str>> {
        use ShaderParamTypeKind::*;

        match param.param_type() {
            Unknown => throw!("Cannot add an effect param of unknown type."),
            Bool  => self.params_bool.push(EffectParamCustomBool::new(param.downcast().unwrap(), settings)),
            Float => self.params_float.push(EffectParamCustomFloat::new(param.downcast().unwrap(), settings)),
            Int   => self.params_int.push(EffectParamCustomInt::new(param.downcast().unwrap(), settings)),
            Vec2 | Vec3 | Vec4 | IVec2 | IVec3 | IVec4 | Mat4 => {
                throw!("Multi-component types as effect params are not yet supported.");
            },
            String => throw!("Strings as effect params are not yet supported."),
            Texture => throw!("Textures as effect params are not yet supported."),
        }

        Ok(())
    }
}

struct EffectParams {
    elapsed_time: EffectParam<ShaderParamTypeFloat>,
    uv_size: EffectParam<ShaderParamTypeIVec2>,
    custom: EffectParamsCustom,
}

impl EffectParams {
    fn assign_values(&mut self) {
        self.elapsed_time.assign_value();
        self.uv_size.assign_value();
        self.custom.assign_values();
    }
}

struct PreparedEffect {
    effect: GraphicsEffect,
    params: EffectParams,
}

struct Data {
    source: SourceContext,
    effect: Option<PreparedEffect>,
    creation: Instant,

    property_shader: PropertyDescriptor<PropertyDescriptorSpecializationPath>,
    property_shader_reload: PropertyDescriptor<PropertyDescriptorSpecializationButton>,

    settings_update_requested: Arc<AtomicBool>,
}

impl Data {
    pub fn new(settings: &mut SettingsContext, source: SourceContext) -> Self {
        let settings_update_requested = Arc::new(AtomicBool::new(true));

        Self {
            source,
            effect: None,
            creation: Instant::now(),
            property_shader: PropertyDescriptor {
                name: CString::new("shader").unwrap(),
                description: CString::new("The shader to use.").unwrap(),
                specialization: PropertyDescriptorSpecializationPath {
                    path_type: PathType::File,
                    filter: CString::from(cstr!("*.glsl *.frag *.fragment ;; All File Types | *.*")),
                    default_path: CString::from(cstr!("")),
                },
            },
            property_shader_reload: PropertyDescriptor {
                name: CString::new("shader_reload").unwrap(),
                description: CString::new("Reload Shader").unwrap(),
                specialization: PropertyDescriptorSpecializationButton::new(
                    Box::new({
                        let settings_update_requested = settings_update_requested.clone();
                        move || {
                            settings_update_requested.store(true, Ordering::SeqCst);
                            false
                        }
                    }),
                )
            },
            settings_update_requested,
        }
    }
}

// impl Drop for Data {
//     fn drop(&mut self) {
//         self.send.send(FilterMessage::CloseConnection).unwrap_or(());
//     }
// }

struct ScrollFocusFilter {
    context: ModuleContext,
}

impl Sourceable for ScrollFocusFilter {
    fn get_id() -> &'static CStr {
        cstr!("obs-shaderfilter-plus")
    }
    fn get_type() -> SourceType {
        SourceType::FILTER
    }
}

impl GetNameSource<Data> for ScrollFocusFilter {
    fn get_name() -> &'static CStr {
        cstr!("ShaderFilter Plus")
    }
}

impl GetPropertiesSource<Data> for ScrollFocusFilter {
    fn get_properties(context: PluginContext<Data>) -> Properties {
        let data = context.data().as_ref().unwrap();
        let mut properties = Properties::new();

        properties.add_property(&data.property_shader);
        properties.add_property(&data.property_shader_reload);

        if let Some(effect) = data.effect.as_ref() {
            effect.params.custom.add_properties(&mut properties);
        }

        properties
    }
}

impl VideoTickSource<Data> for ScrollFocusFilter {
    fn video_tick(mut context: PluginContext<Data>, seconds: f32) {
        let (data, settings) = context.data_settings_mut();
        let data = if let Some(data) = data.as_mut() {
            data
        } else {
            return;
        };

        if let Some(effect) = data.effect.as_mut() {
            let params = &mut effect.params;

            params.elapsed_time.prepare_value(data.creation.elapsed().as_secs_f32());
            params.uv_size.prepare_value([
                data.source.get_base_width() as i32,
                data.source.get_base_height() as i32,
            ]);
        }

        if data.settings_update_requested.compare_and_swap(true, false, Ordering::SeqCst) {
            data.source.update_source_settings(settings);
        }
    }
}

impl VideoRenderSource<Data> for ScrollFocusFilter {
    fn video_render(
        mut context: PluginContext<Data>,
        _context: &mut ActiveContext,
        render: &mut VideoRenderContext,
    ) {
        let data = if let Some(data) = context.data_mut().as_mut() {
            data
        } else {
            return;
        };
        let prepared_effect = if let Some(effect) = data.effect.as_mut() {
            effect
        } else {
            return;
        };
        let effect = &mut prepared_effect.effect;
        let params = &mut prepared_effect.params;
        let source = &mut data.source;
        // let param_add = &mut data.add_val;
        // let param_mul = &mut data.mul_val;
        // let image = &mut data.image;
        // let sampler = &mut data.sampler;

        // let current = &mut data.current;

        // let zoom = data.current_zoom as f32;

        let mut cx: u32 = 1;
        let mut cy: u32 = 1;

        source.do_with_target(|target| {
            cx = target.get_base_width();
            cy = target.get_base_height();
        });

        source.process_filter(
            render,
            effect,
            (cx, cy),
            GraphicsColorFormat::RGBA,
            GraphicsAllowDirectRendering::NoDirectRendering,
            |context, _effect| {
                params.assign_values();
                // params.elapsed_time.set_param_value(seconds_elapsed);
                // param_add.set_vec2(context, &Vec2::new(current.x(), current.y()));
                // param_mul.set_vec2(context, &Vec2::new(zoom, zoom));
                // image.set_next_sampler(context, sampler);
            },
        );
    }
}

impl CreatableSource<Data> for ScrollFocusFilter {
    fn create(settings: &mut SettingsContext, mut source: SourceContext) -> Data {
        // let sampler = GraphicsSamplerState::from(GraphicsSamplerInfo::default());

        // let (send_filter, receive_filter) = unbounded::<FilterMessage>();
        // let (send_server, receive_server) = unbounded::<ServerMessage>();

        // std::thread::spawn(move || {
        //     let mut server = Server::new().unwrap();

        //     loop {
        //         if let Some(snapshot) = server.wait_for_event() {
        //             send_server
        //                 .send(ServerMessage::Snapshot(snapshot))
        //                 .unwrap_or(());
        //         }

        //         if let Ok(msg) = receive_filter.try_recv() {
        //             match msg {
        //                 FilterMessage::CloseConnection => {
        //                     return;
        //                 }
        //             }
        //         }
        //     }
        // });

        // source.update_source_settings(settings);

        Data::new(settings, source)
    }
}

impl UpdateSource<Data> for ScrollFocusFilter {
    fn update(
        mut context: PluginContext<Data>,
        _context: &mut ActiveContext,
    ) {
        let result: Result<(), Cow<str>> = try {
            let (data, settings) = context.data_settings_mut();
            let data = data.as_mut().ok_or_else(|| "Could not access the data.")?;

            const EFFECT_SOURCE_TEMPLATE: &'static str = include_str!("effect_template.effect");
            let shader_path = settings.get_property_value(&data.property_shader, &PathBuf::new());
            let mut shader_file = File::open(&shader_path)
                .map_err(|_| {
                    data.effect = None;
                    format!("Shader not found at the specified path: {:?}", &shader_path)
                })?;

            let mut shader = String::new();
            shader_file.read_to_string(&mut shader).expect("Could not read the shader at the given path.");
            let pattern = Regex::new(r"(?P<shader>__SHADER__)").unwrap();
            let effect_source = pattern.replace_all(EFFECT_SOURCE_TEMPLATE, shader.as_str());

            let shader_path_c = CString::new(
                shader_path.to_str().ok_or_else(|| {
                    "Specified shader path is not a valid UTF-8 string."
                })?
            ).map_err(|_| "Shader path cannot be converted to a C string.")?;
            let effect_source_c = CString::new(effect_source.as_ref())
                .map_err(|_| "Shader contents cannot be converted to a C string.")?;

            let effect = GraphicsEffect::from_effect_string(
                effect_source_c.as_c_str(),
                shader_path_c.as_c_str(),
            ).ok_or_else(|| "Could not create the effect.")?;
            let mut builtin_param_names = vec!["ViewProj", "image"];

            macro_rules! builtin_effect {
                ($path:expr) => {{
                    builtin_param_names.push($path);
                    EffectParam::new(
                        effect.get_param_by_name(cstr!($path))
                            .ok_or_else(|| {
                                format!("Could not access built in effect parameter `{}`.", $path)
                            })?
                            .downcast()
                            .ok_or_else(|| {
                                format!("Incompatible effect parameter type `{}`.", $path)
                            })?
                    )
                }}
            }

            let mut params = EffectParams {
                elapsed_time: builtin_effect!("elapsed_time"),
                uv_size: builtin_effect!("uv_size"),
                custom: Default::default(),
            };

            let custom_params_iter = effect.params_iter().filter(|item| {
                !builtin_param_names.contains(&item.name())
            });

            for custom_param in custom_params_iter {
                params.custom.add_param(custom_param, settings)?;
            }

            data.effect = Some(PreparedEffect {
                effect,
                params,
            });

            data.source.update_source_properties();
        };

        if let Err(error_message) = result {
            println!("An error occurred while updating a ShaderFilter Plus filter: {}", error_message);
        }
    }
}

impl Module for ScrollFocusFilter {
    fn new(context: ModuleContext) -> Self {
        Self { context }
    }
    fn get_ctx(&self) -> &ModuleContext {
        &self.context
    }

    fn load(&mut self, load_context: &mut LoadContext) -> bool {
        let source = load_context
            .create_source_builder::<ScrollFocusFilter, Data>()
            .enable_get_name()
            .enable_create()
            .enable_get_properties()
            .enable_update()
            .enable_video_render()
            .enable_video_tick()
            .build();

        load_context.register_source(source);

        true
    }

    fn description() -> &'static CStr {
        cstr!("A plugin to provide a way of specifying effects using shaders.")
    }

    fn name() -> &'static CStr {
        cstr!("OBS ShaderFilter Plus")
    }

    fn author() -> &'static CStr {
        cstr!("Jakub \"Limeth\" Hlusička, Charles Fettinger, NLeseul")
    }
}

obs_register_module!(ScrollFocusFilter);
