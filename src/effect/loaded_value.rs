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
    pub allow_definitions_in_source: bool,
    pub default_descriptor_specialization: <T as LoadedValueTypePropertyDescriptor>::Specialization,
    pub default_value: <<T as LoadedValueTypePropertyDescriptor>::Specialization as ValuePropertyDescriptorSpecialization>::ValueType,
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
