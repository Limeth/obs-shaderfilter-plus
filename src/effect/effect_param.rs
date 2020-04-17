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
use obs_wrapper::obs_sys::{
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

    pub fn enable_and_drop(self, graphics_context: &GraphicsContext) {
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
