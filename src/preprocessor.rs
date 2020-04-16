use std::str::FromStr;
use std::borrow::Cow;
use std::collections::HashMap;
use regex::{Regex, Replacer, Captures};
use anyhow::Result;
use crate::*;

#[derive(Default)]
pub struct PreprocessResult {
    map: HashMap<(String, String), String>,
}

impl PreprocessResult {
    pub fn parse<T: FromStr>(&self, field: &str, property: &str) -> Option<Result<T, Cow<'static, str>>> {
        self.map.get(&(field.to_string(), property.to_string()))
            .map(|raw| {
                raw.parse::<T>().map_err(|_| {
                    Cow::Owned(format!(
                        "Could not parse property `{}` of builtin field `{}` of type `{}`.",
                        property, field, std::any::type_name::<T>()
                    ))
                })
            })
    }

    pub fn parse_default<T: FromStr>(&self, field: &str, property: &str, default: Option<T>) -> Result<T, Cow<'static, str>> {
        self.parse::<T>(field, property)
            .or_else(|| {
                default.map(|default| Ok(default))
            })
            .ok_or_else(|| {
                Cow::Owned(format!(
                    "Property `{}` of builtin field `{}` is missing a definition.",
                    property, field
                ))
            })
            .and_then(|result| result)
    }
}

impl<'a> Replacer for &'a mut PreprocessResult {
    // Appends the replacement string to `dst`.
    // In our case, however, we want to get rid of all #pragma macros, not replace them.
    fn replace_append(&mut self, caps: &Captures, _dst: &mut String) {
        let result: Result<(), Cow<str>> = try {
            let field = caps.name("field").ok_or_else(|| "Missing field name.")?.as_str().to_string();
            let property = caps.name("property").ok_or_else(|| "Missing property name.")?.as_str().to_string();
            let value = caps.name("value").ok_or_else(|| "Missing value.")?.as_str().to_string();

            self.map.insert(dbg!((field, property)), dbg!(value));
        };

        if let Err(result) = result {
            eprintln!("Could not parse `#pragma shaderfilter`: {}
                Make sure the macro usage follows the form `#pragma shaderfilter <field> <property> <value>`.", result);
        }
    }
}

pub fn preprocess(source: &str) -> (PreprocessResult, Cow<str>) {
    let mut result = PreprocessResult::default();
    // Matches on macros:
    // #pragma shaderfilter <field> <property> <value>
    let pattern = Regex::new(r"(?m)#pragma\s+shaderfilter\s+set\s+(?P<field>\w+)\s+(?P<property>\w+)\s+(?P<value>[^\s].*?)\s*$").unwrap();
    let string = pattern.replace_all(source, &mut result);

    (result, string)
}
