use std::str::FromStr;
use std::borrow::Cow;
use std::collections::HashMap;
use regex::{Regex, Replacer, Captures};
use anyhow::Result;

#[derive(Default)]
pub struct PreprocessResult {
    map: HashMap<String, String>,
}

impl PreprocessResult {
    pub fn parse<T: FromStr>(&self, identifier: &str) -> Option<Result<T, Cow<'static, str>>> {
        self.map.get(identifier)
            .map(|raw| {
                raw.parse::<T>().map_err(|_| {
                    Cow::Owned(format!(
                        "Could not parse property `{}` of type `{}`.",
                        identifier,
                        std::any::type_name::<T>(),
                    ))
                })
            })
    }

    pub fn parse_default<T: FromStr>(&self, identifier: &str, default: Option<T>) -> Result<T, Cow<'static, str>> {
        self.parse::<T>(identifier)
            .or_else(|| {
                default.map(|default| Ok(default))
            })
            .ok_or_else(|| {
                Cow::Owned(format!(
                    "Property `{}` is missing a definition.",
                    identifier,
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
            let identifier = caps.name("identifier").ok_or_else(|| "Missing property identifier.")?.as_str().to_string();
            let value = caps.name("value").ok_or_else(|| "Missing value.")?.as_str().to_string();

            self.map.insert(identifier, value);
        };

        if let Err(result) = result {
            eprintln!("Could not parse `#pragma shaderfilter`: {}
                Make sure the macro usage follows the form `#pragma shaderfilter <identifier> <property> <value>`.", result);
        }
    }
}

pub fn preprocess(source: &str) -> (PreprocessResult, Cow<str>) {
    let mut result = PreprocessResult::default();
    // Matches on macros:
    // #pragma shaderfilter <identifier> <value>
    let pattern = Regex::new(r"(?m)^\s*#pragma\s+shaderfilter\s+set\s+(?P<identifier>\w+)\s+(?P<value>[^\s].*?)\s*$").unwrap();
    let string = pattern.replace_all(source, &mut result);

    (result, string)
}
