#![feature(try_blocks)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock, Arc, Weak};
use std::borrow::Cow;
use std::time::{Instant, Duration};
use std::path::PathBuf;
use std::fs::File;
use std::ffi::{CStr, CString};
use std::io::Read;
use ammolite_math::*;
use smallvec::{SmallVec, smallvec};
use lazy_static::lazy_static;
use obs_wrapper::{
    context::*,
    graphics::*,
    obs_register_module,
    prelude::*,
    source::*,
    audio::*,
};
use regex::Regex;
use fourier::*;
use num_complex::Complex;
use effect::*;

macro_rules! throw {
    ($e:expr) => {{
        Err($e)?;
        unreachable!()
    }}
}

mod effect;

lazy_static! {
    static ref GLOBAL_STATE: GlobalState = Default::default();
}

pub trait GlobalStateComponentType {
    type Descriptor;
    type Result;

    fn create(descriptor: &Self::Descriptor) -> Self;
    fn register_callback(self: &Arc<Self>, callback: Box<dyn Fn(&Self::Result) + Send + Sync>);
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct GlobalStateAudioFFTDescriptor {
    mix: usize,
    // Assume stereo signal, channels in ascending order
    channels: SmallVec<[usize; 2]>,
}

impl GlobalStateAudioFFTDescriptor {
    pub fn new(mix: usize, channels: &[usize]) -> Self {
        let mut channels_vec = SmallVec::with_capacity(channels.len());

        channels_vec.extend_from_slice(channels);
        channels_vec.sort();

        // TODO: freeze channels_vec

        Self {
            mix,
            channels: channels_vec,
        }
    }
}

pub struct GlobalStateAudioFFTMutable {
    callbacks: Vec<Box<dyn Fn(&Vec<f32>) + Send + Sync>>,
    audio_output: Option<AudioOutput>,
}

impl Default for GlobalStateAudioFFTMutable {
    fn default() -> Self {
        Self {
            callbacks: Default::default(),
            audio_output: Default::default(),
        }
    }
}

pub struct GlobalStateAudioFFT {
    descriptor: GlobalStateAudioFFTDescriptor,
    mutable: Arc<RwLock<GlobalStateAudioFFTMutable>>,
}

impl GlobalStateAudioFFT {
    fn process_audio_data<'a>(self: &Arc<Self>, audio_data: AudioData<'a>) -> <Self as GlobalStateComponentType>::Result {
        match audio_data.info().format() {
            AudioFormatKind::PlanarF32 => {
                let audio_data = audio_data.downcast::<AudioFormatPlanarF32>().unwrap();
                let mut average = vec![0.0; audio_data.inner.frames() as usize];

                self.descriptor.channels.iter().copied()
                    .for_each(|channel| {
                        audio_data.samples(channel).map(|samples| {
                            let len = samples.len();
                            let mut fft_data: Vec<Complex<f32>> = samples.map(|sample| {
                                Complex::new(sample, 0.0)
                            }).collect::<Vec<_>>();
                            let fft = fourier::create_fft_f32(len);

                            fft.transform_in_place(&mut fft_data, Transform::SqrtScaledFft);

                            fft_data.into_iter().enumerate().for_each(|(sample_index, complex)| {
                                let norm = complex.norm();

                                average[sample_index] += norm;
                            });
                        });
                    });

                average.iter_mut().for_each(|value| *value = *value / self.descriptor.channels.len() as f32);

                return average
            },
            _ => {
                eprintln!("Unsupported audio format.");
                return Vec::new();
            }
        }
    }
}

impl GlobalStateComponentType for GlobalStateAudioFFT {
    type Descriptor = GlobalStateAudioFFTDescriptor;
    type Result = Vec<f32>; // TODO

    fn create(descriptor: &Self::Descriptor) -> Self {
        Self {
            descriptor: descriptor.clone(),
            mutable: Default::default(),
        }
    }

    fn register_callback(self: &Arc<Self>, callback: Box<dyn Fn(&Self::Result) + Send + Sync>) {
        let mut mutable_write = self.mutable.write().unwrap();
        let connect_output = mutable_write.callbacks.len() == 0;

        mutable_write.callbacks.push(callback);

        if connect_output {
            let audio = Audio::get();

            mutable_write.audio_output = Some(audio.connect_output(
                self.descriptor.mix,
                {
                    let mutable = self.mutable.clone();
                    let self_cloned = self.clone();

                    Box::new(move |audio_data| {
                        let result = Self::process_audio_data(&self_cloned, audio_data);
                        let mutable_read = mutable.read().unwrap();

                        for callback in &mutable_read.callbacks {
                            (callback)(&result);
                        }
                    })
                },
            ))
        }
    }
}

/// A component of the global state, which is dynamically allocated and
/// deallocated depending on the reference count.
#[derive(Default)]
pub struct GlobalStateComponent<T: GlobalStateComponentType> {
    pub weak_ptr: RwLock<Weak<T>>,
    pub descriptor: T::Descriptor,
}

impl<T: GlobalStateComponentType> GlobalStateComponent<T> {
    pub fn new(descriptor: T::Descriptor) -> Self {
        Self {
            weak_ptr: RwLock::new(Weak::new()),
            descriptor,
        }
    }

    /// Attempts to get a strong reference to the component.
    /// If the component was freed, it is constructed by this function.
    pub fn get_component(&self) -> Arc<T> {
        {
            let weak_ptr_read = self.weak_ptr.read().unwrap();

            if let Some(strong_ptr) = weak_ptr_read.upgrade() {
                return strong_ptr;
            }
        }

        {
            let mut weak_ptr_write = self.weak_ptr.write().unwrap();

            if let Some(strong_ptr) = weak_ptr_write.upgrade() {
                return strong_ptr;
            }

            let strong_ptr: Arc<T> = Arc::new(T::create(&self.descriptor));

            *weak_ptr_write = Arc::downgrade(&strong_ptr.clone());

            strong_ptr
        }
    }

    pub fn try_get_component(&self) -> Option<Arc<T>> {
        let weak_ptr_read = self.weak_ptr.read().unwrap();

        weak_ptr_read.upgrade()
    }
}

pub struct GlobalState {
    pub audio_ffts: RwLock<HashMap<GlobalStateAudioFFTDescriptor, GlobalStateComponent<GlobalStateAudioFFT>>>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            audio_ffts: Default::default(),
        }
    }
}

impl GlobalState {
    fn request_audio_fft(&self, descriptor: &GlobalStateAudioFFTDescriptor) -> Arc<GlobalStateAudioFFT> {
        {
            let audio_ffts_read = self.audio_ffts.read().unwrap();

            if let Some(audio_fft) = audio_ffts_read.get(descriptor) {
                return audio_fft.get_component();
            }
        }

        {
            let mut audio_ffts_write = self.audio_ffts.write().unwrap();

            if let Some(audio_fft) = audio_ffts_write.get(descriptor) {
                return audio_fft.get_component();
            }

            let component_wrapper = GlobalStateComponent::new(descriptor.clone());
            let component = component_wrapper.get_component();

            audio_ffts_write.insert(descriptor.clone(), component_wrapper);

            component
        }
    }
}

// use crossbeam_channel::{unbounded, Receiver, Sender};

struct Data {
    source: SourceContext,
    effect: Option<PreparedEffect>,
    creation: Instant,

    audio_fft: Arc<GlobalStateAudioFFT>,
    texture_fft: Arc<Mutex<Option<TextureDescriptor>>>,

    property_shader: PropertyDescriptor<PropertyDescriptorSpecializationPath>,
    property_shader_reload: PropertyDescriptor<PropertyDescriptorSpecializationButton>,

    settings_update_requested: Arc<AtomicBool>,
}

impl Data {
    pub fn new(source: SourceContext) -> Self {
        let settings_update_requested = Arc::new(AtomicBool::new(true));
        let audio_fft = GLOBAL_STATE.request_audio_fft(&GlobalStateAudioFFTDescriptor::new(
            0, // mix
            &[0, 1], // channels
        ));
        let texture_fft: Arc<Mutex<Option<TextureDescriptor>>> = Default::default();

        audio_fft.register_callback({
            let texture_fft = texture_fft.clone();

            Box::new(move |result| {
                let mut texture_fft = texture_fft.lock().unwrap();
                let texture_data = unsafe {
                    std::slice::from_raw_parts::<u8>(
                        result.as_ptr() as *const _,
                        result.len() * std::mem::size_of::<f32>(),
                    )
                }.iter().copied().collect::<Vec<_>>();
                let texture = TextureDescriptor {
                    dimensions: [result.len(), 1],
                    color_format: ColorFormatKind::R32F,
                    levels: smallvec![texture_data],
                    flags: 0,
                };

                *texture_fft = Some(texture);
            })
        });

        Self {
            source,
            effect: None,
            creation: Instant::now(),
            audio_fft,
            texture_fft,
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

impl Drop for Data {
    fn drop(&mut self) {
        // self.send.send(FilterMessage::CloseConnection).unwrap_or(());
        if let Some(prepared_effect) = self.effect.take() {
            let graphics_context = GraphicsContext::enter().unwrap();
            prepared_effect.enable_and_drop(&graphics_context);
        }
    }
}

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

            {
                let mut texture_fft = data.texture_fft.lock().unwrap();

                if let Some(texture_fft) = texture_fft.take() {
                    // println!("Preparing texture");
                    // dbg!(&texture_fft.levels[0][0..10]);
                    params.texture_fft.prepare_value(texture_fft);
                }
            }

            // {
            //     let graphics_context = GraphicsContext::enter().unwrap();
            //     params.stage_values(&graphics_context);
            // }
        }

        if data.settings_update_requested.compare_and_swap(true, false, Ordering::SeqCst) {
            data.source.update_source_settings(settings);
        }
    }
}

impl VideoRenderSource<Data> for ScrollFocusFilter {
    fn video_render(
        mut context: PluginContext<Data>,
        graphics_context: &mut GraphicsContext,
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
        let effect = &mut prepared_effect.effect.as_enabled_mut(graphics_context);
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
            effect,
            (cx, cy),
            ColorFormatKind::RGBA,
            GraphicsAllowDirectRendering::NoDirectRendering,
            |context, _effect| {
                params.stage_values(&context);
                params.assign_values(&context);
                // image.set_next_sampler(context, sampler);
            },
        );
    }
}

impl CreatableSource<Data> for ScrollFocusFilter {
    fn create(settings: &mut SettingsContext, source: SourceContext) -> Data {
        Data::new(source)
    }
}

impl UpdateSource<Data> for ScrollFocusFilter {
    fn update(
        mut context: PluginContext<Data>,
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

            let graphics_context = GraphicsContext::enter()
                .expect("Could not enter a graphics context.");
            let effect = GraphicsEffect::from_effect_string(
                effect_source_c.as_c_str(),
                shader_path_c.as_c_str(),
                &graphics_context,
            ).ok_or_else(|| "Could not create the effect.")?;
            let mut builtin_param_names = vec!["ViewProj", "image"];

            effect.params_iter().for_each(|param| {
                dbg!(param.name());
            });

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
                            .disable()
                    )
                }}
            }

            let mut params = EffectParams {
                elapsed_time: builtin_effect!("elapsed_time"),
                uv_size: builtin_effect!("uv_size"),
                texture_fft: builtin_effect!("texture_fft"),
                custom: Default::default(),
            };

            let custom_params_iter = effect.params_iter().filter(|item| {
                !builtin_param_names.contains(&item.name())
            });

            for custom_param in custom_params_iter {
                params.custom.add_param(custom_param, settings)?;
            }

            data.effect.replace(PreparedEffect {
                effect: effect.disable(),
                params,
            }).map(|original| {
                original.enable_and_drop(&graphics_context);
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
