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

/// Used to convert cloneable values into `ShaderParamType::RustType`.
pub trait EffectParamType {
    type ShaderParamType: ShaderParamType;
    type PreparedValueType: Default;

    fn convert_and_stage_value(
        prepared: Self::PreparedValueType,
        context: &GraphicsContext,
    ) -> GraphicsContextDependentDisabled<<Self::ShaderParamType as ShaderParamType>::RustType>;

    fn assign_staged_value<'a, 'b>(
        staged: &'b <Self::ShaderParamType as ShaderParamType>::RustType,
        param: &'b mut EnableGuardMut<'a, 'b, GraphicsEffectParamTyped<Self::ShaderParamType>, GraphicsContext>,
        context: &'b FilterContext,
    );
}

macro_rules! define_effect_param_aliases {
    ($($name:ident),*$(,)?) => {
        item! {
            $(
                pub type [< EffectParam $name >] = EffectParam<EffectParamTypeClone<[< ShaderParamType $name >]>>;
            )*
        }
    }
}

define_effect_param_aliases! {
    Bool, Int, IVec2, IVec3, IVec4, Float, Vec2, Vec3, Vec4, Mat4,
}

#[derive(Clone, Debug)]
pub struct TextureDescriptor {
    pub dimensions: [usize; 2],
    pub color_format: ColorFormatKind,
    pub levels: SmallVec<[Vec<u8>; 1]>,
    pub flags: u32,
}

impl Default for TextureDescriptor {
    fn default() -> Self {
        Self {
            dimensions: [1, 1],
            color_format: ColorFormatKind::RGBA,
            levels: smallvec![vec![0, 0, 0, 0]],
            flags: 0,
        }
    }
}

pub type EffectParamTexture = EffectParam<EffectParamTypeTexture>;

pub struct EffectParamTypeClone<T>
    where T: ShaderParamType,
          <T as ShaderParamType>::RustType: Default,
{
    __marker: std::marker::PhantomData<T>,
}

impl<T> EffectParamType for EffectParamTypeClone<T>
    where T: ShaderParamType,
          <T as ShaderParamType>::RustType: Default,
{
    type ShaderParamType = T;
    type PreparedValueType = <T as ShaderParamType>::RustType;

    fn convert_and_stage_value(
        prepared: Self::PreparedValueType,
        context: &GraphicsContext,
    ) -> GraphicsContextDependentDisabled<<Self::ShaderParamType as ShaderParamType>::RustType> {
        ContextDependent::new(prepared, context).disable()
    }

    fn assign_staged_value<'a, 'b>(
        staged: &'b <Self::ShaderParamType as ShaderParamType>::RustType,
        param: &'b mut EnableGuardMut<'a, 'b, GraphicsEffectParamTyped<Self::ShaderParamType>, GraphicsContext>,
        context: &'b FilterContext,
    ) {
        param.set_param_value(&staged, context);
    }
}

pub struct EffectParamTypeTexture;

impl EffectParamType for EffectParamTypeTexture {
    type ShaderParamType = ShaderParamTypeTexture;
    type PreparedValueType = TextureDescriptor;

    fn convert_and_stage_value(
        prepared: Self::PreparedValueType,
        context: &GraphicsContext,
    ) -> GraphicsContextDependentDisabled<<Self::ShaderParamType as ShaderParamType>::RustType> {
        let levels: Vec<&[u8]> = prepared.levels.iter().map(|vec| &vec[..]).collect::<Vec<_>>();

        Texture::new(
            prepared.dimensions,
            prepared.color_format,
            &levels,
            prepared.flags,
            context,
        ).disable()
    }

    fn assign_staged_value<'a, 'b>(
        staged: &'b <Self::ShaderParamType as ShaderParamType>::RustType,
        param: &'b mut EnableGuardMut<'a, 'b, GraphicsEffectParamTyped<Self::ShaderParamType>, GraphicsContext>,
        context: &'b FilterContext,
    ) {
        param.set_param_value(&staged, context);
    }
}

/// This type takes care of three different tasks:
/// It stores a _prepared value_ (see `prepare_value`).
/// It creates a graphics resource from the _prepared value_, if it was changed, and stores the result (see `stage_value`).
/// It assigns the staged values to filters (see `assign_value`).
pub struct EffectParam<T: EffectParamType> {
    pub param: GraphicsContextDependentDisabled<GraphicsEffectParamTyped<<T as EffectParamType>::ShaderParamType>>,
    pub prepared_value: Option<T::PreparedValueType>,
    pub staged_value: Option<GraphicsContextDependentDisabled<<<T as EffectParamType>::ShaderParamType as ShaderParamType>::RustType>>,
}

impl<T: EffectParamType> EffectParam<T> {
    pub fn new(param: GraphicsContextDependentDisabled<GraphicsEffectParamTyped<<T as EffectParamType>::ShaderParamType>>) -> Self {
        Self {
            param,
            prepared_value: Some(Default::default()),
            staged_value: None,
        }
    }

    /// Requests a new staged value to be generated from this prepared value
    pub fn prepare_value(&mut self, new_value: T::PreparedValueType) {
        self.prepared_value = Some(new_value);
    }

    /// If a value is prepared (not `None`), creates a graphics resource from that value,
    /// to be used in effect filter processing.
    pub fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        if let Some(prepared_value) = self.prepared_value.take() {
            if let Some(previous) = self.staged_value.replace(<T as EffectParamType>::convert_and_stage_value(
                prepared_value,
                graphics_context,
            )) {
                previous.enable(graphics_context);
            }
        }
    }

    /// Assigns the staged value to the effect current filter.
    pub fn assign_value<'a>(&mut self, context: &'a FilterContext) {
        let staged_value = self.staged_value.as_ref()
            .expect("Tried to assign a value before staging it.")
            .as_enabled(context.graphics());

        <T as EffectParamType>::assign_staged_value(
            &staged_value,
            &mut self.param.as_enabled_mut(context.graphics()),
            context,
        );
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.param.enable(graphics_context);
    }
}

pub trait EffectParamCustom {
    type ShaderParamType: ShaderParamType;
    type PropertyDescriptorSpecialization: PropertyDescriptorSpecialization;

    fn new<'a>(param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>, settings: &mut SettingsContext) -> Self;
    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext);
    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext);
    fn enable_and_drop(self, graphics_context: &GraphicsContext);
}

pub struct EffectParamCustomBool {
    pub effect_param: EffectParamBool,
    pub property_descriptor: PropertyDescriptor<PropertyDescriptorSpecializationBool>,
}

impl EffectParamCustom for EffectParamCustomBool {
    type ShaderParamType = ShaderParamTypeBool;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationBool;

    fn new<'a>(param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>, settings: &mut SettingsContext) -> Self {
        let default_value = *param.get_param_value_default().unwrap_or(&false);
        let mut result = Self {
            property_descriptor: PropertyDescriptor {
                name: CString::new(param.inner.name()).unwrap(),
                description: CString::new(param.inner.name()).unwrap(),
                specialization: Self::PropertyDescriptorSpecialization {},
            },
            effect_param: EffectParam::new(param.disable()),
        };
        let loaded_value = settings.get_property_value(&result.property_descriptor, &default_value);

        result.effect_param.prepare_value(loaded_value);

        result
    }

    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        self.effect_param.assign_value(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.effect_param.enable_and_drop(graphics_context);
    }
}

pub struct EffectParamCustomInt {
    pub effect_param: EffectParamInt,
    pub property_descriptor: PropertyDescriptor<PropertyDescriptorSpecializationI32>,
}

impl EffectParamCustom for EffectParamCustomInt {
    type ShaderParamType = ShaderParamTypeInt;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationI32;

    fn new<'a>(param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>, settings: &mut SettingsContext) -> Self {
        let default_value = *param.get_param_value_default().unwrap_or(&0);
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
            effect_param: EffectParam::new(param.disable()),
        };
        let loaded_value = settings.get_property_value(&result.property_descriptor, &default_value);

        result.effect_param.prepare_value(loaded_value);

        result
    }

    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        self.effect_param.assign_value(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.effect_param.enable_and_drop(graphics_context);
    }
}

pub struct EffectParamCustomFloat {
    pub effect_param: EffectParamFloat,
    pub property_descriptor: PropertyDescriptor<PropertyDescriptorSpecializationF64>,
}

impl EffectParamCustom for EffectParamCustomFloat {
    type ShaderParamType = ShaderParamTypeFloat;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationF64;

    fn new<'a>(param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>, settings: &mut SettingsContext) -> Self {
        let default_value = *param.get_param_value_default().unwrap_or(&0.0) as f64;
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
            effect_param: EffectParam::new(param.disable()),
        };
        let loaded_value = settings.get_property_value(&result.property_descriptor, &default_value);

        result.effect_param.prepare_value(loaded_value as f32);

        result
    }

    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        self.effect_param.assign_value(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.effect_param.enable_and_drop(graphics_context);
    }
}

pub struct EffectParamCustomVec4 {
    pub effect_param: EffectParamVec4,
    pub property_descriptor: PropertyDescriptor<PropertyDescriptorSpecializationColor>,
}

impl EffectParamCustom for EffectParamCustomVec4 {
    type ShaderParamType = ShaderParamTypeVec4;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationColor;

    fn new<'a>(param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>, settings: &mut SettingsContext) -> Self {
        let default_value = *param.get_param_value_default().unwrap_or(&[0.0; 4]);
        let mut result = Self {
            property_descriptor: PropertyDescriptor {
                name: CString::new(param.inner.name()).unwrap(),
                description: CString::new(param.inner.name()).unwrap(),
                specialization: Self::PropertyDescriptorSpecialization {},
            },
            effect_param: EffectParam::new(param.disable()),
        };
        let loaded_value = settings.get_property_value(&result.property_descriptor, &default_value);

        result.effect_param.prepare_value(loaded_value);

        result
    }

    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        self.effect_param.assign_value(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.effect_param.enable_and_drop(graphics_context);
    }
}

pub trait BuiltinType {
    fn name() -> &'static str;

    fn format_builtin_field(field_name: &str, property_name: Option<&str>) -> String {
        if let Some(property_name) = property_name {
            format!("builtin_{}_{}_{}", Self::name(), field_name, property_name)
        } else {
            format!("builtin_{}_{}", Self::name(), field_name)
        }
    }

    fn new_property<T>(field_name: &str, property_name: &str, specialization: T) -> PropertyDescriptor<T>
        where T: PropertyDescriptorSpecialization,
    {
        PropertyDescriptor {
            name: CString::new(Self::format_builtin_field(field_name, Some(property_name))).unwrap(),
            description: CString::new(format!("{} {}", field_name, property_name)).unwrap(),
            specialization,
        }
    }
}

/// A simple property. Its value is assigned from either the shader source code (hardcoded)
/// or from the effect settings properties.
pub struct PropertyKind<T>
where T: ValuePropertyDescriptorSpecialization,
      <T as ValuePropertyDescriptorSpecialization>::ValueType: FromStr + Default + Clone,
{
    setting: Option<PropertyDescriptor<T>>,
    value: <T as ValuePropertyDescriptorSpecialization>::ValueType,
}

impl<T> PropertyKind<T>
where T: ValuePropertyDescriptorSpecialization,
      <T as ValuePropertyDescriptorSpecialization>::ValueType: FromStr + Default + Clone,
{
    pub fn from<B: BuiltinType>(
        allow_definitions_in_source: bool,
        default_value: <T as ValuePropertyDescriptorSpecialization>::ValueType,
        field_name: &str,
        property_name: &str,
        specialization: T,
        preprocess_result: &PreprocessResult,
        settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>> {
        let setting_name = B::format_builtin_field(field_name, Some(property_name));
        let hardcoded_value = preprocess_result.parse::<<T as ValuePropertyDescriptorSpecialization>::ValueType>(field_name, property_name);

        Ok(match hardcoded_value {
            Some(result) if allow_definitions_in_source => {
                Self {
                    setting: None,
                    value: result?,
                }
            },
            _ => {
                let property_name_default = format!("{}_default", property_name);
                let default_value = if allow_definitions_in_source {
                    dbg!(preprocess_result.parse::<<T as ValuePropertyDescriptorSpecialization>::ValueType>(field_name, &property_name_default)
                    .transpose()?)
                } else {
                    None
                }.unwrap_or(default_value);
                let setting = B::new_property(field_name, property_name, specialization);
                let loaded_value = settings.get_property_value(&setting, &default_value);

                Self {
                    setting: Some(setting),
                    value: loaded_value,
                }
            }
        })
    }

    pub fn add_properties(&self, properties: &mut Properties) {
        if let &Some(ref setting) = &self.setting {
            properties.add_property(setting);
        }
    }

    pub fn get_value(&self) -> <T as ValuePropertyDescriptorSpecialization>::ValueType {
        self.value.clone()
    }
}

pub struct EffectParamCustomFFT {
    pub effect_param: EffectParamTexture,
    pub audio_fft: Arc<GlobalStateAudioFFT>,
    pub property_mix: PropertyKind<PropertyDescriptorSpecializationI32>,
    pub property_channel: PropertyKind<PropertyDescriptorSpecializationI32>,
    pub property_dampening_factor_attack: PropertyKind<PropertyDescriptorSpecializationF64>,
    pub property_dampening_factor_release: PropertyKind<PropertyDescriptorSpecializationF64>,
}

impl BuiltinType for EffectParamCustomFFT {
    fn name() -> &'static str {
        "texture_fft"
    }
}

impl EffectParamCustomFFT {
    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<ShaderParamTypeTexture>>,
        field_name: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property_mix = PropertyKind::from::<Self>(
            false,
            1,
            field_name,
            "mix",
            // TODO: make customizable using macros
            PropertyDescriptorSpecializationI32 {
                min: 1,
                max: MAX_AUDIO_MIXES as i32,
                step: 1,
                slider: false,
            },
            preprocess_result,
            settings,
        )?;
        let property_channel = PropertyKind::from::<Self>(
            false,
            1,
            field_name,
            "channel",
            // TODO: make customizable using macros
            PropertyDescriptorSpecializationI32 {
                min: 1,
                max: 2, // FIXME: Currently causes crashes when out of bounds MAX_AUDIO_CHANNELS as i32,
                step: 1,
                slider: false,
            },
            preprocess_result,
            settings,
        )?;
        let property_dampening_factor_attack = PropertyKind::from::<Self>(
            true,
            0.0,
            field_name,
            "dampening_factor_attack",
            // TODO: make customizable using macros
            PropertyDescriptorSpecializationF64 {
                min: 0.0,
                max: 0.0,
                step: 0.1,
                slider: false,
            },
            preprocess_result,
            settings,
        )?;
        let property_dampening_factor_release = PropertyKind::from::<Self>(
            true,
            0.0,
            field_name,
            "dampening_factor_release",
            // TODO: make customizable using macros
            PropertyDescriptorSpecializationF64 {
                min: 0.0,
                max: 0.0,
                step: 0.1,
                slider: false,
            },
            preprocess_result,
            settings,
        )?;

        let audio_fft_descriptor = GlobalStateAudioFFTDescriptor::new(
            property_mix.get_value() as usize - 1,
            property_channel.get_value() as usize - 1,
            property_dampening_factor_attack.get_value(),
            property_dampening_factor_release.get_value(),
            // TODO: Make customizable, but provide a sane default value
            WindowFunction::Hanning,
        );

        Ok(Self {
            effect_param: EffectParam::new(param.disable()),
            audio_fft: GLOBAL_STATE.request_audio_fft(&audio_fft_descriptor),
            property_mix,
            property_channel,
            property_dampening_factor_attack,
            property_dampening_factor_release,
        })
    }

    pub fn add_properties(&self, properties: &mut Properties) {
        self.property_mix.add_properties(properties);
        self.property_channel.add_properties(properties);
        self.property_dampening_factor_attack.add_properties(properties);
        self.property_dampening_factor_release.add_properties(properties);
    }

    fn prepare_values<'a>(&mut self) {
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
        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        self.effect_param.assign_value(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.effect_param.enable_and_drop(graphics_context);
    }
}

#[derive(Default)]
pub struct EffectParamsCustom {
    pub params_bool: Vec<EffectParamCustomBool>,
    pub params_float: Vec<EffectParamCustomFloat>,
    pub params_int: Vec<EffectParamCustomInt>,
    pub params_vec4: Vec<EffectParamCustomVec4>,
    pub params_fft: Vec<EffectParamCustomFFT>,
    // TODO: Textures
}

impl EffectParamsCustom {
    pub fn prepare_values(&mut self) {
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
        self.params_bool.iter().for_each(|param| properties.add_property(&param.property_descriptor));
        self.params_float.iter().for_each(|param| properties.add_property(&param.property_descriptor));
        self.params_int.iter().for_each(|param| properties.add_property(&param.property_descriptor));
        self.params_vec4.iter().for_each(|param| properties.add_property(&param.property_descriptor));
        self.params_fft.iter().for_each(|param| param.add_properties(properties));
    }

    pub fn add_param<'a>(
        &mut self,
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParam>,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<(), Cow<'static, str>> {
        use ShaderParamTypeKind::*;

        let param_name = param.name().to_string();
        let result: Result<(), Cow<'static, str>> = try {
            {
                let pattern_builtin_texture_fft = Regex::new(r"^builtin_texture_fft_(?P<field>\w+)$").unwrap();

                if let Some(captures) = pattern_builtin_texture_fft.captures(&param_name) {
                    let field_name = captures.name("field").unwrap().as_str();

                    if param.param_type() != Texture {
                        throw!(format!("Builtin field `{}` must be of type `{}`", field_name, "texture2d"));
                    }

                    self.params_fft.push(EffectParamCustomFFT::new(
                        param.downcast().unwrap(),
                        field_name,
                        settings,
                        preprocess_result,
                    )?);
                    return Ok(());
                }
            }

            match param.param_type() {
                Unknown => throw!("Cannot add an effect param of unknown type. Make sure to use HLSL type names for uniform variables."),
                Bool  => self.params_bool.push(EffectParamCustomBool::new(param.downcast().unwrap(), settings)),
                Float => self.params_float.push(EffectParamCustomFloat::new(param.downcast().unwrap(), settings)),
                Int   => self.params_int.push(EffectParamCustomInt::new(param.downcast().unwrap(), settings)),
                Vec4  => self.params_vec4.push(EffectParamCustomVec4::new(param.downcast().unwrap(), settings)),
                Vec2 | Vec3 | IVec2 | IVec3 | IVec4 | Mat4 => {
                    throw!("Multi-component types as effect params are not yet supported.");
                },
                String => throw!("Strings as effect params are not yet supported."),
                Texture => throw!("Textures as effect params are not yet supported."),
            }
        };

        result.map_err(|err| {
            Cow::Owned(format!("An error occurred while binding effect uniform variable `{}`: {}", param_name, err))
        })
    }
}

pub struct EffectParams {
    pub elapsed_time: EffectParamFloat,
    pub uv_size: EffectParamIVec2,
    pub texture_fft: EffectParamTexture,
    pub custom: EffectParamsCustom,
}

impl EffectParams {
    pub fn stage_values(&mut self, graphics_context: &GraphicsContext) {
        self.elapsed_time.stage_value(graphics_context);
        self.uv_size.stage_value(graphics_context);
        self.texture_fft.stage_value(graphics_context);
        self.custom.stage_values(graphics_context);
    }

    pub fn assign_values(&mut self, graphics_context: &FilterContext) {
        self.elapsed_time.assign_value(graphics_context);
        self.uv_size.assign_value(graphics_context);
        self.texture_fft.assign_value(graphics_context);
        self.custom.assign_values(graphics_context);
    }

    pub fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.elapsed_time.enable_and_drop(graphics_context);
        self.uv_size.enable_and_drop(graphics_context);
        self.texture_fft.enable_and_drop(graphics_context);
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
