#![feature(try_blocks)]
#![feature(c_variadic)]
#![feature(trait_alias)]
#![feature(associated_type_bounds)]

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{RwLock, Arc, Weak};
use std::borrow::Cow;
use std::time::Instant;
use std::path::PathBuf;
use std::fs::File;
use std::ffi::{CStr, CString};
use std::io::Read;
use ordered_float::OrderedFloat;
use lazy_static::lazy_static;
use obs_wrapper::{
    info::*,
    context::*,
    graphics::*,
    obs_register_module,
    prelude::*,
    source::*,
    audio::*,
};
use fourier::*;
use num_complex::Complex;
use util::*;
use effect::*;
use preprocessor::*;

macro_rules! throw {
    ($e:expr) => {{
        Err($e)?;
        unreachable!()
    }}
}

mod util;
mod effect;
mod preprocessor;

lazy_static! {
    static ref GLOBAL_STATE: GlobalState = Default::default();
}

pub trait GlobalStateComponentType {
    type Descriptor;
    type Result;

    fn create(descriptor: &Self::Descriptor) -> Arc<Self>;
    fn retrieve_result(self: &Arc<Self>) -> Option<Self::Result>;
    // fn register_callback(self: &Arc<Self>, callback: Box<dyn Fn(&Self::Result) + Send + Sync>);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowFunction {
    None,
    Blackman,
    Cosine {
        a: OrderedFloat<f64>,
        b: OrderedFloat<f64>,
        c: OrderedFloat<f64>,
        d: OrderedFloat<f64>,
    },
    Hamming,
    Hanning,
    Nuttall,
    Triangular,
}

impl WindowFunction {
    pub fn generate_coefficients(self, len: usize) -> Vec<f32> {
        use apodize::*;
        use WindowFunction::*;

        match self {
            None => std::iter::repeat(1.0).take(len).collect::<Vec<_>>(),
            Blackman => blackman_iter(len).map(|coef| coef as f32).collect::<Vec<_>>(),
            Cosine { a, b, c, d } => cosine_iter(*a, *b, *c, *d, len).map(|coef| coef as f32).collect::<Vec<_>>(),
            Hamming => hamming_iter(len).map(|coef| coef as f32).collect::<Vec<_>>(),
            Hanning => hanning_iter(len).map(|coef| coef as f32).collect::<Vec<_>>(),
            Nuttall => nuttall_iter(len).map(|coef| coef as f32).collect::<Vec<_>>(),
            Triangular => triangular_iter(len).map(|coef| coef as f32).collect::<Vec<_>>(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GlobalStateAudioFFTDescriptor {
    mix: usize,
    channel: usize,
    dampening_factor_attack: OrderedFloat<f64>,
    dampening_factor_release: OrderedFloat<f64>,
    window_function: WindowFunction,
}

impl GlobalStateAudioFFTDescriptor {
    pub fn new(
        mix: usize,
        channel: usize,
        dampening_factor_attack: f64,
        dampening_factor_release: f64,
        window_function: WindowFunction,
    ) -> Self {
        Self {
            mix,
            channel,
            dampening_factor_attack: OrderedFloat(dampening_factor_attack),
            dampening_factor_release: OrderedFloat(dampening_factor_release),
            window_function,
        }
    }
}

#[derive(Clone)]
pub struct FFTResult {
    batch_number: usize,
    frequency_spectrum: Arc<Vec<f32>>,
}

pub struct GlobalStateAudioFFTMutable {
    audio_output: Option<AudioOutput>,
    sample_buffer: VecDeque<f32>,
    window: Arc<Vec<f32>>,
    /// Set during `retrieve_result` to indicate that the analysis of the next
    /// batch should be performed.
    next_batch_scheduled: AtomicBool,
    /// The result of the analysis.
    result: Option<FFTResult>,
}

impl Default for GlobalStateAudioFFTMutable {
    fn default() -> Self {
        Self {
            audio_output: Default::default(),
            sample_buffer: Default::default(),
            window: Arc::new(Vec::new()),
            next_batch_scheduled: AtomicBool::new(true),
            result: None,
        }
    }
}

pub struct GlobalStateAudioFFT {
    descriptor: GlobalStateAudioFFTDescriptor,
    mutable: Arc<RwLock<GlobalStateAudioFFTMutable>>,
}

impl GlobalStateAudioFFT {
    fn get_samples_per_frame() -> usize {
        let audio_info = ObsAudioInfo::get()
            .expect("Audio info not accessible.");
        let video_info = ObsVideoInfo::get()
            .expect("Video info not accessible.");
        let framerate = video_info.framerate();

        (audio_info.samples_per_second() as usize * framerate.denominator as usize)
            / framerate.numerator as usize
    }

    fn render_frames_to_time_elapsed(render_frames: usize) -> f64 {
        let video_info = ObsVideoInfo::get()
            .expect("Video info not accessible.");
        let framerate = video_info.framerate();

        (render_frames as f64 * framerate.denominator as f64) / framerate.numerator as f64
    }

    fn perform_analysis(
        samples: impl Iterator<Item=f32> + ExactSizeIterator,
        window: &[f32],
    ) -> Vec<f32> {
        assert_eq!(samples.len(), window.len());

        let len = samples.len();
        let mut fft_data: Vec<Complex<f32>> = samples.zip(window.iter()).map(|(sample, window_coefficient)| {
            Complex::new(sample * window_coefficient, 0.0)
        }).collect::<Vec<_>>();
        let fft = fourier::create_fft_f32(len);

        fft.transform_in_place(&mut fft_data, Transform::Fft);

        fft_data.into_iter().take(len / 2).map(|complex| {
            // normalize according to https://www.sjsu.edu/people/burford.furman/docs/me120/FFT_tutorial_NI.pdf
            (complex.norm() * 4.0 / len as f32).sqrt()
        }).collect::<Vec<_>>()
    }

    fn process_audio_data<'a>(this: &Weak<Self>, audio_data: AudioData<'a, ()>) {
        let this = if let Some(this) = Weak::upgrade(this) {
            this
        } else {
            // The audio FFT component no longer exists, bail.
            return;
        };

        let mut mutable_write = this.mutable.write().unwrap();

        let current_samples = if let Some(samples) = audio_data.samples_normalized(this.descriptor.channel) {
            samples
        } else {
            // No samples captured, bail.
            return;
        };

        mutable_write.sample_buffer.extend(current_samples);

        let samples_per_frame = Self::get_samples_per_frame();
        let render_frames_accumulated = mutable_write.sample_buffer.len() / samples_per_frame;
        let render_frames_over_margin = render_frames_accumulated.saturating_sub(1);

        // Get rid of old data, if we lost some frames, or if the results are not being requested.
        if render_frames_over_margin > 0 {
            // println!("Skipping {} render frames in FFT calculation.", render_frames_over_margin);

            let samples_to_remove = samples_per_frame * render_frames_over_margin;
            mutable_write.sample_buffer.drain(0..samples_to_remove);
        }

        if !mutable_write.next_batch_scheduled.load(Ordering::SeqCst) {
            return;
        }

        if render_frames_accumulated > 0 {
            if mutable_write.window.len() != samples_per_frame {
                mutable_write.window = Arc::new(this.descriptor.window_function.generate_coefficients(samples_per_frame));
            }

            let window = mutable_write.window.clone();
            let current_accumulated_samples = mutable_write.sample_buffer.drain(0..samples_per_frame);
            let mut analysis_result = Self::perform_analysis(current_accumulated_samples, &window);

            // Dampen the result by mixing it with the result from the previous batch
            if *this.descriptor.dampening_factor_attack > 0.0 || *this.descriptor.dampening_factor_release > 0.0 {
                if let Some(previous_result) = mutable_write.result.as_ref() {
                    let dampening_multiplier_attack = this.descriptor.dampening_factor_attack.powf(
                        Self::render_frames_to_time_elapsed(render_frames_accumulated)
                    ).clamp(0.0, 1.0) as f32;
                    let dampening_multiplier_release = this.descriptor.dampening_factor_release.powf(
                        Self::render_frames_to_time_elapsed(render_frames_accumulated)
                    ).clamp(0.0, 1.0) as f32;

                    analysis_result.iter_mut()
                        .zip(previous_result.frequency_spectrum.iter())
                        .for_each(move |(current, previous)| {
                            let dampening_multiplier = if *current > *previous {
                                dampening_multiplier_attack
                            } else {
                                dampening_multiplier_release
                            };

                            *current = dampening_multiplier * *previous + (1.0 - dampening_multiplier) * *current;
                        })
                }
            }

            let next_batch_number = mutable_write.result.as_ref()
                .map(|result| result.batch_number + 1).unwrap_or(0);
            mutable_write.result = Some(FFTResult {
                batch_number: next_batch_number,
                frequency_spectrum: Arc::new(analysis_result),
            });
            mutable_write.next_batch_scheduled.swap(false, Ordering::SeqCst);
        }
    }
}

impl GlobalStateComponentType for GlobalStateAudioFFT {
    type Descriptor = GlobalStateAudioFFTDescriptor;
    type Result = FFTResult;

    fn create(descriptor: &Self::Descriptor) -> Arc<Self> {
        let audio = Audio::get();
        let result = Arc::new(Self {
            descriptor: descriptor.clone(),
            mutable: Default::default(),
        });

        let audio_output = audio.connect_output(
            descriptor.mix,
            {
                let self_cloned = Arc::downgrade(&result);

                Box::new(move |audio_data| {
                    Self::process_audio_data(&self_cloned, audio_data);
                })
            },
        );

        result.mutable.write().unwrap().audio_output = Some(audio_output);

        result
    }

    fn retrieve_result(self: &Arc<Self>) -> Option<Self::Result> {
        let mutable_read = self.mutable.read().unwrap();

        mutable_read.next_batch_scheduled.store(true, Ordering::SeqCst);
        mutable_read.result.clone()
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

            let strong_ptr: Arc<T> = T::create(&self.descriptor);

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

            audio_ffts_write.retain(|_, component| component.try_get_component().is_some());
            audio_ffts_write.insert(descriptor.clone(), component_wrapper);

            component
        }
    }
}

// use crossbeam_channel::{unbounded, Receiver, Sender};

struct Data {
    source: SourceContext,
    effect: Option<PreparedEffect>,
    effect_fallback_blit: GraphicsContextDependentDisabled<GraphicsEffect>,

    creation: Instant,
    shown: Option<Instant>,
    next_frame: u32,
    elapsed_time_previous: Option<f32>,
    elapsed_time_since_shown_previous: Option<f32>,

    property_shader: PropertyDescriptor<PropertyDescriptorSpecializationPath>,
    property_shader_reload: PropertyDescriptor<PropertyDescriptorSpecializationButton>,
    property_message: PropertyDescriptor<PropertyDescriptorSpecializationString>,
    property_message_display: bool,

    settings_update_requested: Arc<AtomicBool>,
}

impl Data {
    pub fn new(source: SourceContext) -> Self {
        let settings_update_requested = Arc::new(AtomicBool::new(true));

        Self {
            source,
            effect: None,
            effect_fallback_blit: {
                const EFFECT_SOURCE_FALLBACK: &'static str = include_str!("effect_fallback.effect");

                let graphics_context = GraphicsContext::enter()
                    .expect("Could not enter the graphics context to initialize the fallback effect.");

                let shader_path_c = CString::new("effect_fallback.effect").unwrap();
                let effect_source_c = CString::new(EFFECT_SOURCE_FALLBACK)
                    .expect("Shader contents cannot be converted to a C string.");

                GraphicsEffect::from_effect_string(
                    effect_source_c.as_c_str(),
                    shader_path_c.as_c_str(),
                    &graphics_context,
                ).unwrap().disable()
            },
            creation: Instant::now(),
            shown: None,
            next_frame: 0,
            elapsed_time_previous: None,
            elapsed_time_since_shown_previous: None,
            property_shader: PropertyDescriptor {
                name: CString::new("builtin_ui_shader").unwrap(),
                description: CString::new("The shader to use.").unwrap(),
                specialization: PropertyDescriptorSpecializationPath {
                    path_type: PathType::File,
                    filter: CString::from(cstr!("*.hlsl *.glsl *.frag *.fragment ;; All File Types | *.*")),
                    default_path: CString::from(cstr!("")),
                },
            },
            property_shader_reload: PropertyDescriptor {
                name: CString::new("builtin_ui_shader_reload").unwrap(),
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
            property_message: PropertyDescriptor {
                name: CString::new("builtin_ui_message").unwrap(),
                description: CString::new("").unwrap(),
                specialization: PropertyDescriptorSpecializationString {
                    string_type: StringType::Multiline,
                }
            },
            property_message_display: false,
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

struct ShaderFilterPlus {
    context: ModuleContext,
}

impl Sourceable for ShaderFilterPlus {
    fn get_id() -> &'static CStr {
        cstr!("obs-shaderfilter-plus")
    }
    fn get_type() -> SourceType {
        SourceType::FILTER
    }
}

impl GetNameSource<Data> for ShaderFilterPlus {
    fn get_name() -> &'static CStr {
        cstr!("ShaderFilter Plus")
    }
}

impl GetPropertiesSource<Data> for ShaderFilterPlus {
    /// Convention for naming properties, so that they do not conflict:
    /// `builtin_ui_*` -- UI-only properties (not sent to the GPU), like the shader path
    /// `builtin_*` -- Properties either sent to the GPU or influencing other properties which are sent to the GPU
    /// `*` --  Custom uniform properties defined by the user, must not begin with `builtin_`
    fn get_properties(context: PluginContext<Data>) -> Properties {
        let data = context.data().as_ref().unwrap();
        let mut properties = Properties::new();

        properties.add_property(&data.property_shader);
        properties.add_property(&data.property_shader_reload);

        if data.property_message_display {
            properties.add_property(&data.property_message);
        }

        if let Some(effect) = data.effect.as_ref() {
            effect.add_properties(&mut properties);
        }

        properties
    }
}

impl VideoTickSource<Data> for ShaderFilterPlus {
    fn video_tick(mut context: PluginContext<Data>, _seconds: f32) {
        let (data, settings) = context.data_settings_mut();
        let data = if let Some(data) = data.as_mut() {
            data
        } else {
            return;
        };

        let frame = data.next_frame;
        data.next_frame += 1;
        let framerate = ObsVideoInfo::get().map(|info| info.framerate().as_f32()).unwrap_or(0.0);
        let elapsed_time = data.creation.elapsed().as_secs_f32();
        let elapsed_time_previous = data.elapsed_time_previous.replace(elapsed_time)
            .unwrap_or(elapsed_time);
        let elapsed_time_since_shown = if let Some(shown) = data.shown.as_ref() {
            shown.elapsed().as_secs_f32()
        } else {
            data.shown = Some(Instant::now());
            0.0
        };
        let elapsed_time_since_shown_previous = data.elapsed_time_since_shown_previous
            .replace(elapsed_time_since_shown)
            .unwrap_or(elapsed_time_since_shown);

        if let Some(effect) = data.effect.as_mut() {
            let params = &mut effect.params;

            params.frame.prepare_value(frame as i32);
            params.framerate.prepare_value(framerate);
            params.elapsed_time.prepare_value(elapsed_time);
            params.elapsed_time_previous.prepare_value(elapsed_time_previous);
            params.elapsed_time_since_shown.prepare_value(elapsed_time_since_shown);
            params.elapsed_time_since_shown_previous.prepare_value(elapsed_time_since_shown_previous);
            params.uv_size.prepare_value([
                data.source.get_base_width() as i32,
                data.source.get_base_height() as i32,
            ]);

            params.custom.prepare_values();

            {
                let graphics_context = GraphicsContext::enter().unwrap();
                params.stage_values(&graphics_context);
            }
        }

        if data.settings_update_requested.compare_and_swap(true, false, Ordering::SeqCst) {
            data.source.update_source_settings(settings);
        }
    }
}

impl VideoRenderSource<Data> for ShaderFilterPlus {
    fn video_render(
        mut context: PluginContext<Data>,
        graphics_context: &mut GraphicsContext,
    ) {
        let data = if let Some(data) = context.data_mut().as_mut() {
            data
        } else {
            return;
        };

        let source = &mut data.source;

        let mut cx: u32 = 1;
        let mut cy: u32 = 1;

        source.do_with_target(|target| {
            cx = target.get_base_width();
            cy = target.get_base_height();
        });

        let prepared_effect = if let Some(effect) = data.effect.as_mut() {
            effect
        } else { // use the fallback effect
            source.process_filter(
                &mut data.effect_fallback_blit.as_enabled_mut(graphics_context),
                (cx, cy),
                ColorFormatKind::RGBA,
                GraphicsAllowDirectRendering::NoDirectRendering,
                |_context, _effect| {},
            );
            return;
        };

        let effect = &mut prepared_effect.effect.as_enabled_mut(graphics_context);
        let params = &mut prepared_effect.params;

        source.process_filter(
            effect,
            (cx, cy),
            ColorFormatKind::RGBA,
            GraphicsAllowDirectRendering::NoDirectRendering,
            |context, _effect| {
                params.assign_values(&context);
                // image.set_next_sampler(context, sampler);
            },
        );
    }
}

impl CreatableSource<Data> for ShaderFilterPlus {
    fn create(_settings: &mut SettingsContext, source: SourceContext) -> Data {
        Data::new(source)
    }
}

impl UpdateSource<Data> for ShaderFilterPlus {
    fn update(
        mut context: PluginContext<Data>,
    ) {
        let result: Result<(), Cow<str>> = try {
            let (data, mut settings) = context.data_settings_mut();
            let data = data.as_mut().ok_or_else(|| "Could not access the data.")?;

            let shader_path = settings.get_property_value(&data.property_shader, &PathBuf::new());

            if shader_path.as_path().as_os_str().is_empty() {
                throw!("Please specify the shader source file.");
            }

            let mut shader_file = File::open(&shader_path)
                .map_err(|_| {
                    if let Some(effect) = data.effect.take() {
                        let graphics_context = GraphicsContext::enter()
                            .expect("Could not enter a graphics context.");

                        effect.enable_and_drop(&graphics_context);
                    }

                    format!("Shader not found at the specified path: {:?}", &shader_path)
                })?;
            let shader_source = {
                let mut shader_source = String::new();
                shader_file.read_to_string(&mut shader_source).expect("Could not read the shader at the given path.");
                shader_source
            };
            let old_shader_source = data.effect.as_ref().map(|old_effect| {
                old_effect.shader_source.clone()
            });

            if old_shader_source.is_some() && old_shader_source.unwrap() == shader_source {
                // Only update the params, if the shader stayed the same
                let effect = data.effect.as_mut().unwrap();
                effect.params.reload_settings(&mut settings);
                return;
            }

            println!("Shader source changed, recreating effect.");

            // If shader source changed, create a new effect and request to update properties
            let graphics_context = GraphicsContext::enter().unwrap();
            let (effect, preprocess_result) = PreparedEffect::create_effect(&shader_path, &shader_source, &graphics_context)?;
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
                            .disable()
                    )
                }}
            }

            let mut params = EffectParams {
                frame: builtin_effect!("builtin_frame"),
                framerate: builtin_effect!("builtin_framerate"),
                elapsed_time: builtin_effect!("builtin_elapsed_time"),
                elapsed_time_previous: builtin_effect!("builtin_elapsed_time_previous"),
                elapsed_time_since_shown: builtin_effect!("builtin_elapsed_time_since_shown"),
                elapsed_time_since_shown_previous: builtin_effect!("builtin_elapsed_time_since_shown_previous"),
                uv_size: builtin_effect!("builtin_uv_size"),
                custom: Default::default(),
            };

            let custom_params = effect.params_iter()
                .filter(|item| {
                    !builtin_param_names.contains(&item.name())
                })
                .enumerate()
                .map(|(index, param)| {
                    (param.name().to_string(), Indexed::from((index, param)))
                })
                .collect::<HashMap<_, _>>();

            params.custom = EffectParamsCustom::from(custom_params, settings, &preprocess_result)?;

            let effect = PreparedEffect {
                effect: effect.disable(),
                shader_source: shader_source.clone(),
                params,
            };

            // Drop old effect before the new one is created.
            if let Some(old_effect) = data.effect.replace(effect) {
                let graphics_context = GraphicsContext::enter()
                    .expect("Could not enter a graphics context.");
                old_effect.enable_and_drop(&graphics_context);
            }

            data.property_message_display = false;

            settings.set_property_value(&data.property_message, CString::new("").unwrap());
            data.source.update_source_properties();
        };

        if let Err(error_message) = result {
            println!("An error occurred while updating a ShaderFilter Plus filter: {}", error_message);

            let (data, settings) = context.data_settings_mut();

            if let Some(data) = data.as_mut() {
                data.property_message_display = true;

                settings.set_property_value(
                    &data.property_message,
                    CString::new(error_message.as_ref()).unwrap(),
                );
                data.source.update_source_properties();
            }
        }
    }
}

impl HideSource<Data> for ShaderFilterPlus {
    fn hide(mut context: PluginContext<Data>) {
        if let Some(data) = context.data_mut().as_mut() {
            data.shown = None;
        }
    }
}

impl Module for ShaderFilterPlus {
    fn new(context: ModuleContext) -> Self {
        Self { context }
    }
    fn get_ctx(&self) -> &ModuleContext {
        &self.context
    }

    fn load(&mut self, load_context: &mut LoadContext) -> bool {
        let source = load_context
            .create_source_builder::<ShaderFilterPlus, Data>()
            .enable_get_name()
            .enable_create()
            .enable_get_properties()
            .enable_update()
            .enable_video_render()
            .enable_video_tick()
            .enable_hide()
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

obs_register_module!(ShaderFilterPlus);
