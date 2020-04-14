use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::borrow::Cow;
use std::time::{Instant, Duration};
use std::path::PathBuf;
use std::fs::File;
use std::ffi::{CStr, CString};
use std::io::Read;
use ammolite_math::*;
use obs_wrapper::{
    graphics::*,
    obs_register_module,
    prelude::*,
    source::*,
    context::*,
};
use smallvec::{SmallVec, smallvec};
use regex::Regex;
use paste::item;

/// Used to convert cloneable values into `ShaderParamType::RustType`.
pub trait EffectParamType {
    type ShaderParamType: ShaderParamType;
    type PreparedValueType: Default + Clone;

    fn convert_and_stage_value(
        prepared: Self::PreparedValueType,
        context: &FilterContext,
    ) -> FilterContextDependentDisabled<<Self::ShaderParamType as ShaderParamType>::RustType>;

    fn assign_staged_value<'a, 'b>(
        staged: &'b <Self::ShaderParamType as ShaderParamType>::RustType,
        param: &'b mut EnableGuardMut<'a, 'b, GraphicsEffectParamTyped<Self::ShaderParamType>, GraphicsContext>,
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
          <T as ShaderParamType>::RustType: Clone + Default,
{
    __marker: std::marker::PhantomData<T>,
}

impl<T> EffectParamType for EffectParamTypeClone<T>
    where T: ShaderParamType,
          <T as ShaderParamType>::RustType: Clone + Default,
{
    type ShaderParamType = T;
    type PreparedValueType = <T as ShaderParamType>::RustType;

    fn convert_and_stage_value(
        prepared: Self::PreparedValueType,
        context: &FilterContext,
    ) -> FilterContextDependentDisabled<<Self::ShaderParamType as ShaderParamType>::RustType> {
        ContextDependent::new(prepared, context).disable()
    }

    fn assign_staged_value<'a, 'b>(
        staged: &'b <Self::ShaderParamType as ShaderParamType>::RustType,
        param: &'b mut EnableGuardMut<'a, 'b, GraphicsEffectParamTyped<Self::ShaderParamType>, GraphicsContext>,
    ) {
        param.set_param_value(&staged);
    }
}

pub struct EffectParamTypeTexture;

impl EffectParamType for EffectParamTypeTexture {
    type ShaderParamType = ShaderParamTypeTexture;
    type PreparedValueType = TextureDescriptor;

    fn convert_and_stage_value(
        prepared: Self::PreparedValueType,
        context: &FilterContext,
    ) -> FilterContextDependentDisabled<<Self::ShaderParamType as ShaderParamType>::RustType> {
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
    ) {
        param.set_param_value(&staged);
    }
}

pub struct EffectParam<T: EffectParamType> {
    pub param: GraphicsContextDependentDisabled<GraphicsEffectParamTyped<<T as EffectParamType>::ShaderParamType>>,
    pub prepared_value: T::PreparedValueType,
    pub staged_value: Option<FilterContextDependentDisabled<<<T as EffectParamType>::ShaderParamType as ShaderParamType>::RustType>>,
}

impl<T: EffectParamType> EffectParam<T> {
    pub fn new(param: GraphicsContextDependentDisabled<GraphicsEffectParamTyped<<T as EffectParamType>::ShaderParamType>>) -> Self {
        Self {
            param,
            prepared_value: Default::default(),
            staged_value: None,
        }
    }

    pub fn prepare_value(&mut self, new_value: T::PreparedValueType) {
        self.prepared_value = new_value;
    }

    pub fn stage_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        if let Some(previous) = self.staged_value.replace(<T as EffectParamType>::convert_and_stage_value(
            self.prepared_value.clone(),
            graphics_context,
        )) {
            previous.enable(graphics_context);
        }
    }

    pub fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        let staged_value = self.staged_value.as_ref()
            .expect("Tried to assign a value before staging it.")
            .as_enabled(graphics_context);
        <T as EffectParamType>::assign_staged_value(
            &staged_value,
            &mut self.param.as_enabled_mut(graphics_context.graphics()),
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
    fn stage_value<'a>(&mut self, graphics_context: &'a FilterContext);
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
        let default_value = *param.get_param_value_default();
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

    fn stage_value<'a>(&mut self, graphics_context: &'a FilterContext) {
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
        let default_value = *param.get_param_value_default();
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

    fn stage_value<'a>(&mut self, graphics_context: &'a FilterContext) {
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
        let default_value = *param.get_param_value_default() as f64;
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

    fn stage_value<'a>(&mut self, graphics_context: &'a FilterContext) {
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
    // TODO: Textures
}

impl EffectParamsCustom {
    pub fn stage_values(&mut self, graphics_context: &FilterContext) {
        self.params_bool.iter_mut().for_each(|param| param.stage_value(graphics_context));
        self.params_float.iter_mut().for_each(|param| param.stage_value(graphics_context));
        self.params_int.iter_mut().for_each(|param| param.stage_value(graphics_context));
    }

    pub fn assign_values(&mut self, graphics_context: &FilterContext) {
        self.params_bool.iter_mut().for_each(|param| param.assign_value(graphics_context));
        self.params_float.iter_mut().for_each(|param| param.assign_value(graphics_context));
        self.params_int.iter_mut().for_each(|param| param.assign_value(graphics_context));
    }

    pub fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.params_bool.into_iter().for_each(|param| param.enable_and_drop(graphics_context));
        self.params_float.into_iter().for_each(|param| param.enable_and_drop(graphics_context));
        self.params_int.into_iter().for_each(|param| param.enable_and_drop(graphics_context));
    }

    pub fn add_properties(&self, properties: &mut Properties) {
        self.params_bool.iter().for_each(|param| properties.add_property(&param.property_descriptor));
        self.params_float.iter().for_each(|param| properties.add_property(&param.property_descriptor));
        self.params_int.iter().for_each(|param| properties.add_property(&param.property_descriptor));
    }

    pub fn add_param<'a>(&mut self, param: GraphicsContextDependentEnabled<'a, GraphicsEffectParam>, settings: &mut SettingsContext) -> Result<(), Cow<'static, str>> {
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

pub struct EffectParams {
    pub elapsed_time: EffectParamFloat,
    pub uv_size: EffectParamIVec2,
    pub texture_fft: EffectParamTexture,
    pub custom: EffectParamsCustom,
}

impl EffectParams {
    pub fn stage_values(&mut self, graphics_context: &FilterContext) {
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
}
