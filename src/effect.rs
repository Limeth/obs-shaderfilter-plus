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

    pub fn stage_value_custom<'a>(&mut self, value: GraphicsContextDependentDisabled<<<T as EffectParamType>::ShaderParamType as ShaderParamType>::RustType>, graphics_context: &'a GraphicsContext) {
        if let Some(previous) = self.staged_value.replace(value) {
            previous.enable(graphics_context);
        }
    }

    pub fn take_staged_value(&mut self) -> Option<GraphicsContextDependentDisabled<<<T as EffectParamType>::ShaderParamType as ShaderParamType>::RustType>> {
        self.staged_value.take()
    }

    /// Assigns the staged value to the effect current filter.
    /// Keeps the staged value around.
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

    pub fn assign_value_if_staged<'a>(&mut self, context: &'a FilterContext) {
        if self.staged_value.is_some() {
            self.assign_value(context);
        }
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.param.enable(graphics_context);
        if let Some(staged_value) = self.staged_value {
            staged_value.enable(graphics_context);
        }
    }
}

pub trait EffectParamCustom: Sized {
    type ShaderParamType: ShaderParamType;
    type PropertyDescriptorSpecialization: PropertyDescriptorSpecialization;

    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>>;
    fn add_properties(&self, properties: &mut Properties);
    fn prepare_values(&mut self);
    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext);
    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext);
    fn enable_and_drop(self, graphics_context: &GraphicsContext);
}

pub struct EffectParamCustomBool {
    pub effect_param: EffectParamBool,
    pub property: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorBool>
}

impl EffectParamCustom for EffectParamCustomBool {
    type ShaderParamType = ShaderParamTypeBool;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationBool;

    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: *param.get_param_value_default().unwrap_or(&false),
                default_descriptor_specialization: PropertyDescriptorSpecializationBool {},
            },
            identifier,
            None,
            preprocess_result,
            settings,
        )?;
        let mut effect_param = EffectParam::new(param.disable());

        effect_param.prepare_value(property.get_value());

        Ok(Self {
            property,
            effect_param,
        })
    }

    fn add_properties(&self, properties: &mut Properties) {
        self.property.add_properties(properties);
    }

    fn prepare_values(&mut self) {}

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
    pub property: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorI32>
}

impl EffectParamCustom for EffectParamCustomInt {
    type ShaderParamType = ShaderParamTypeInt;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationI32;

    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: *param.get_param_value_default().unwrap_or(&0),
                default_descriptor_specialization: Self::PropertyDescriptorSpecialization {
                    min: std::i32::MIN,
                    max: std::i32::MAX,
                    step: 1,
                    slider: false,
                },
            },
            identifier,
            None,
            preprocess_result,
            settings,
        )?;
        let mut effect_param = EffectParam::new(param.disable());

        effect_param.prepare_value(property.get_value());

        Ok(Self {
            property,
            effect_param,
        })
    }

    fn add_properties(&self, properties: &mut Properties) {
        self.property.add_properties(properties);
    }

    fn prepare_values(&mut self) {}

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
    pub property: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorF64>
}

impl EffectParamCustom for EffectParamCustomFloat {
    type ShaderParamType = ShaderParamTypeFloat;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationF64;

    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: *param.get_param_value_default().unwrap_or(&0.0) as f64,
                default_descriptor_specialization: Self::PropertyDescriptorSpecialization {
                    min: std::f64::MIN,
                    max: std::f64::MAX,
                    step: 0.1,
                    slider: false,
                },
            },
            identifier,
            None,
            preprocess_result,
            settings,
        )?;
        let mut effect_param = EffectParam::new(param.disable());

        effect_param.prepare_value(property.get_value() as f32);

        Ok(Self {
            property,
            effect_param,
        })
    }

    fn add_properties(&self, properties: &mut Properties) {
        self.property.add_properties(properties);
    }

    fn prepare_values(&mut self) {}

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

pub struct EffectParamCustomColor {
    pub effect_param: EffectParamVec4,
    pub property: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorColor>
}

impl EffectParamCustom for EffectParamCustomColor {
    type ShaderParamType = ShaderParamTypeVec4;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationColor;

    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: Color(*param.get_param_value_default().unwrap_or(&[0.0; 4])),
                default_descriptor_specialization: Self::PropertyDescriptorSpecialization {},
            },
            identifier,
            None,
            preprocess_result,
            settings,
        )?;
        let mut effect_param = EffectParam::new(param.disable());

        effect_param.prepare_value((property.get_value() as Color).into());

        Ok(Self {
            property,
            effect_param,
        })
    }

    fn add_properties(&self, properties: &mut Properties) {
        self.property.add_properties(properties);
    }

    fn prepare_values(&mut self) {}

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

/// An object representing a binding of setting-properties to graphics uniforms.
pub trait BindableProperty {
    fn add_properties(&self, properties: &mut Properties);
    fn prepare_values(&mut self);
    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext);
    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext);
    fn enable_and_drop(self, graphics_context: &GraphicsContext);
}

pub trait LoadedValueType: Sized {
    type Output;
    type Args;

    fn from_identifier(
        args: Self::Args,
        identifier: &str,
        preprocess_result: &PreprocessResult,
        settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>>;

    fn from(
        args: Self::Args,
        parent_identifier: &str,
        property_name: Option<&str>,
        preprocess_result: &PreprocessResult,
        settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>> {
        if let Some(property_name) = property_name {
            Self::from_identifier(
                args,
                &format!("{}__{}", parent_identifier, property_name),
                preprocess_result,
                settings,
            )
        } else {
            Self::from_identifier(
                args,
                parent_identifier,
                preprocess_result,
                settings,
            )
        }
    }

    fn add_properties(&self, properties: &mut Properties);

    fn get_value(&self) -> Self::Output;
}

pub struct LoadedValueTypeSourceArgs<T> where T: FromStr + Clone {
    default_value: Option<T>,
}

/// A loaded value of which the value can only be specified in the shader source code.
/// Returns `Some(T)`, if a default value is specified.
/// Otherwise, may return `None`.
pub struct LoadedValueTypeSource<T> where T: FromStr + Clone {
    value: Option<T>,
}

impl<T> LoadedValueType for LoadedValueTypeSource<T> where T: FromStr + Clone {
    type Output = Option<T>;
    type Args = LoadedValueTypeSourceArgs<T>;

    fn from_identifier(
        args: Self::Args,
        identifier: &str,
        preprocess_result: &PreprocessResult,
        _settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>> {
        let value = preprocess_result.parse::<T>(identifier)
            .transpose()?
            .or(args.default_value);

        Ok(Self { value })
    }

    fn add_properties(&self, _properties: &mut Properties) {
        // Simple property types do not produce any UI
    }

    fn get_value(&self) -> Self::Output {
        self.value.clone()
    }
}

pub struct LoadedValueTypePropertyDescriptorArgs<T: PropertyDescriptorSpecialization> {
    default_value: T,
}

pub trait LoadedValueTypePropertyDescriptor: Sized {
    type Specialization: PropertyDescriptorSpecialization;

    fn new_args(specialization: Self::Specialization) -> LoadedValueTypePropertyDescriptorArgs<Self::Specialization> {
        LoadedValueTypePropertyDescriptorArgs {
            default_value: specialization,
        }
    }

    fn from_identifier(
        args: LoadedValueTypePropertyDescriptorArgs<Self::Specialization>,
        identifier: &str,
        preprocess_result: &PreprocessResult,
        settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>>;

    fn add_properties(&self, properties: &mut Properties);

    fn get_value(&self) -> PropertyDescriptor<Self::Specialization>;
}

impl<T, L> LoadedValueType for L
where T: PropertyDescriptorSpecialization + Sized,
      L: LoadedValueTypePropertyDescriptor<Specialization=T>,
{
    type Output = PropertyDescriptor<T>;
    type Args = LoadedValueTypePropertyDescriptorArgs<T>;

    fn from_identifier(
        args: Self::Args,
        identifier: &str,
        preprocess_result: &PreprocessResult,
        settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>> {
        <Self as LoadedValueTypePropertyDescriptor>::from_identifier(args, identifier, preprocess_result, settings)
    }

    fn add_properties(&self, properties: &mut Properties) {
        <Self as LoadedValueTypePropertyDescriptor>::add_properties(self, properties)
    }

    fn get_value(&self) -> Self::Output {
        <Self as LoadedValueTypePropertyDescriptor>::get_value(self)
    }
}

/// A loaded value type, which loads a property descriptor from the shader source code.
/// Useful for creating loaded values which retrieve their data from the effect UI.
pub struct LoadedValueTypePropertyDescriptorF64 {
    descriptor: PropertyDescriptor<PropertyDescriptorSpecializationF64>,
    description: LoadedValueTypeSource<String>,
    min: LoadedValueTypeSource<f64>,
    max: LoadedValueTypeSource<f64>,
    step: LoadedValueTypeSource<f64>,
    slider: LoadedValueTypeSource<bool>,
}

impl LoadedValueTypePropertyDescriptor for LoadedValueTypePropertyDescriptorF64 {
    type Specialization = PropertyDescriptorSpecializationF64;

    fn from_identifier(
        args: LoadedValueTypePropertyDescriptorArgs<Self::Specialization>,
        identifier: &str,
        preprocess_result: &PreprocessResult,
        settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>> {
        let description = <LoadedValueTypeSource::<String> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(identifier.to_string()),
            },
            identifier,
            Some("description"),
            preprocess_result,
            settings,
        )?;
        let min = <LoadedValueTypeSource::<f64> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(args.default_value.min),
            },
            identifier,
            Some("min"),
            preprocess_result,
            settings,
        )?;
        let max = <LoadedValueTypeSource::<f64> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(args.default_value.max),
            },
            identifier,
            Some("max"),
            preprocess_result,
            settings,
        )?;
        let step = <LoadedValueTypeSource::<f64> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(args.default_value.step),
            },
            identifier,
            Some("step"),
            preprocess_result,
            settings,
        )?;
        let slider = <LoadedValueTypeSource::<bool> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(args.default_value.slider),
            },
            identifier,
            Some("slider"),
            preprocess_result,
            settings,
        )?;
        let descriptor = PropertyDescriptor {
            // we can safely unwrap loaded values, because default values were specified
            name: CString::new(identifier).unwrap(),
            description: CString::new(description.get_value().unwrap()).unwrap(),
            specialization: PropertyDescriptorSpecializationF64 {
                min: min.get_value().unwrap(),
                max: max.get_value().unwrap(),
                step: step.get_value().unwrap(),
                slider: slider.get_value().unwrap(),
            },
        };

        Ok(Self {
            descriptor,
            description,
            min,
            max,
            step,
            slider,
        })
    }

    fn add_properties(&self, properties: &mut Properties) {
        self.min.add_properties(properties);
        self.max.add_properties(properties);
        self.step.add_properties(properties);
        self.slider.add_properties(properties);
        self.description.add_properties(properties);
        properties.add_property(&self.descriptor);
    }

    fn get_value(&self) -> PropertyDescriptor<Self::Specialization> {
        self.descriptor.clone()
    }
}

pub struct LoadedValueTypePropertyDescriptorI32 {
    descriptor: PropertyDescriptor<PropertyDescriptorSpecializationI32>,
    description: LoadedValueTypeSource<String>,
    min: LoadedValueTypeSource<i32>,
    max: LoadedValueTypeSource<i32>,
    step: LoadedValueTypeSource<i32>,
    slider: LoadedValueTypeSource<bool>,
}

impl LoadedValueTypePropertyDescriptor for LoadedValueTypePropertyDescriptorI32 {
    type Specialization = PropertyDescriptorSpecializationI32;

    fn from_identifier(
        args: LoadedValueTypePropertyDescriptorArgs<Self::Specialization>,
        identifier: &str,
        preprocess_result: &PreprocessResult,
        settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>> {
        let description = <LoadedValueTypeSource::<String> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(identifier.to_string()),
            },
            identifier,
            Some("description"),
            preprocess_result,
            settings,
        )?;
        let min = <LoadedValueTypeSource::<i32> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(args.default_value.min),
            },
            identifier,
            Some("min"),
            preprocess_result,
            settings,
        )?;
        let max = <LoadedValueTypeSource::<i32> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(args.default_value.max),
            },
            identifier,
            Some("max"),
            preprocess_result,
            settings,
        )?;
        let step = <LoadedValueTypeSource::<i32> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(args.default_value.step),
            },
            identifier,
            Some("step"),
            preprocess_result,
            settings,
        )?;
        let slider = <LoadedValueTypeSource::<bool> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(args.default_value.slider),
            },
            identifier,
            Some("slider"),
            preprocess_result,
            settings,
        )?;
        let descriptor = PropertyDescriptor {
            // we can safely unwrap loaded values, because default values were specified
            name: CString::new(identifier).unwrap(),
            description: CString::new(description.get_value().unwrap()).unwrap(),
            specialization: PropertyDescriptorSpecializationI32 {
                min: min.get_value().unwrap(),
                max: max.get_value().unwrap(),
                step: step.get_value().unwrap(),
                slider: slider.get_value().unwrap(),
            },
        };

        Ok(Self {
            descriptor,
            description,
            min,
            max,
            step,
            slider,
        })
    }

    fn add_properties(&self, properties: &mut Properties) {
        self.min.add_properties(properties);
        self.max.add_properties(properties);
        self.step.add_properties(properties);
        self.slider.add_properties(properties);
        self.description.add_properties(properties);
        properties.add_property(&self.descriptor);
    }

    fn get_value(&self) -> PropertyDescriptor<Self::Specialization> {
        self.descriptor.clone()
    }
}

pub struct LoadedValueTypePropertyDescriptorColor {
    descriptor: PropertyDescriptor<PropertyDescriptorSpecializationColor>,
    description: LoadedValueTypeSource<String>,
}

impl LoadedValueTypePropertyDescriptor for LoadedValueTypePropertyDescriptorColor {
    type Specialization = PropertyDescriptorSpecializationColor;

    fn from_identifier(
        args: LoadedValueTypePropertyDescriptorArgs<Self::Specialization>,
        identifier: &str,
        preprocess_result: &PreprocessResult,
        settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>> {
        let description = <LoadedValueTypeSource::<String> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(identifier.to_string()),
            },
            identifier,
            Some("description"),
            preprocess_result,
            settings,
        )?;
        let descriptor = PropertyDescriptor {
            // we can safely unwrap loaded values, because default values were specified
            name: CString::new(identifier).unwrap(),
            description: CString::new(description.get_value().unwrap()).unwrap(),
            specialization: PropertyDescriptorSpecializationColor {},
        };

        Ok(Self {
            descriptor,
            description,
        })
    }

    fn add_properties(&self, properties: &mut Properties) {
        self.description.add_properties(properties);
        properties.add_property(&self.descriptor);
    }

    fn get_value(&self) -> PropertyDescriptor<Self::Specialization> {
        self.descriptor.clone()
    }
}

pub struct LoadedValueTypePropertyDescriptorBool {
    descriptor: PropertyDescriptor<PropertyDescriptorSpecializationBool>,
    description: LoadedValueTypeSource<String>,
}

impl LoadedValueTypePropertyDescriptor for LoadedValueTypePropertyDescriptorBool {
    type Specialization = PropertyDescriptorSpecializationBool;

    fn from_identifier(
        _args: LoadedValueTypePropertyDescriptorArgs<Self::Specialization>,
        identifier: &str,
        preprocess_result: &PreprocessResult,
        settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>> {
        let description = <LoadedValueTypeSource::<String> as LoadedValueType>::from(
            LoadedValueTypeSourceArgs {
                default_value: Some(identifier.to_string()),
            },
            identifier,
            Some("description"),
            preprocess_result,
            settings,
        )?;
        let descriptor = PropertyDescriptor {
            name: CString::new(identifier).unwrap(),
            description: CString::new(description.get_value().unwrap()).unwrap(),
            specialization: PropertyDescriptorSpecializationBool {},
        };

        Ok(Self {
            descriptor,
            description,
        })
    }

    fn add_properties(&self, properties: &mut Properties) {
        self.description.add_properties(properties);
        properties.add_property(&self.descriptor);
    }

    fn get_value(&self) -> PropertyDescriptor<Self::Specialization> {
        self.descriptor.clone()
    }
}

pub struct LoadedValueTypePropertyArgs<T>
where T: LoadedValueTypePropertyDescriptor,
      <T as LoadedValueTypePropertyDescriptor>::Specialization: ValuePropertyDescriptorSpecialization,
      <<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType: FromStr + Clone,
{
    allow_definitions_in_source: bool,
    default_descriptor_specialization: <T as LoadedValueTypePropertyDescriptor>::Specialization,
    default_value: <<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType,
}

/// Represents a hierarchically-loaded value.
/// This value can be either provided by the shader source code,
/// or from the effect settings properties.
pub struct LoadedValueTypeProperty<T>
where T: LoadedValueTypePropertyDescriptor,
      <T as LoadedValueTypePropertyDescriptor>::Specialization: ValuePropertyDescriptorSpecialization,
      <<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType: FromStr + Clone,
{
    loaded_value_descriptor: Option<T>,
    loaded_value_default: Option<LoadedValueTypeSource::<<<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType>>,
    value: <<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType,
}

impl<T> LoadedValueType for LoadedValueTypeProperty<T>
where T: LoadedValueTypePropertyDescriptor,
      <T as LoadedValueTypePropertyDescriptor>::Specialization: ValuePropertyDescriptorSpecialization,
      <<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType: FromStr + Clone,
{
    type Output = <<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType;
    type Args = LoadedValueTypePropertyArgs<T>;

    fn from_identifier(
        args: Self::Args,
        identifier: &str,
        preprocess_result: &PreprocessResult,
        settings: &mut SettingsContext,
    ) -> Result<Self, Cow<'static, str>> {
        let hardcoded_value = preprocess_result.parse::<<<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType>(identifier);

        Ok(match hardcoded_value {
            Some(result) => {
                if !args.allow_definitions_in_source {
                    throw!(format!("The value of the property `{}` may not be hardcoded in the shader source code.", identifier));
                }

                Self {
                    loaded_value_descriptor: None,
                    loaded_value_default: None,
                    value: result?,
                }
            },
            None => {
                let (default_value, loaded_value_default) = if args.allow_definitions_in_source {
                    let loaded_value_default = <LoadedValueTypeSource::<<<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType> as LoadedValueType>::from(
                        LoadedValueTypeSourceArgs {
                            default_value: Some(args.default_value),
                        },
                        identifier,
                        Some("default"),
                        preprocess_result,
                        settings,
                    )?;
                    (loaded_value_default.get_value().unwrap(), Some(loaded_value_default))
                } else {
                    let loaded_value_default = <LoadedValueTypeSource::<<<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType> as LoadedValueType>::from(
                        LoadedValueTypeSourceArgs {
                            default_value: None,
                        },
                        identifier,
                        Some("default"),
                        preprocess_result,
                        settings,
                    );

                    if loaded_value_default.is_err()
                        || loaded_value_default.unwrap().get_value().is_some() {
                            throw!(format!("The default value of the property `{}` may not be hardcoded in the shader source code.", identifier))
                    }

                    (args.default_value, None)
                };
                let loaded_value_descriptor = <T as LoadedValueType>::from(
                    <T as LoadedValueTypePropertyDescriptor>::new_args(args.default_descriptor_specialization),
                    identifier,
                    None,
                    preprocess_result,
                    settings,
                )?;
                let descriptor = loaded_value_descriptor.get_value();
                let loaded_value = settings.get_property_value(&descriptor, &default_value);

                Self {
                    loaded_value_descriptor: Some(loaded_value_descriptor),
                    loaded_value_default,
                    value: loaded_value,
                }
            }
        })
    }

    fn add_properties(&self, properties: &mut Properties) {
        if let &Some(ref default) = &self.loaded_value_default {
            default.add_properties(properties);
        }
        if let &Some(ref descriptor) = &self.loaded_value_descriptor {
            descriptor.add_properties(properties);
        }
    }

    fn get_value(&self) -> <<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType {
        self.value.clone()
    }
}

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
