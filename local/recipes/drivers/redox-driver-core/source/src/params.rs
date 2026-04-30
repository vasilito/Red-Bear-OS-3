use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamValue {
    Bool(bool),
    Int(i64),
    Uint(u64),
    String(String),
    Enum(String, Vec<String>),
}

impl ParamValue {
    pub fn type_name(&self) -> &str {
        match self {
            ParamValue::Bool(_) => "bool",
            ParamValue::Int(_) => "int",
            ParamValue::Uint(_) => "uint",
            ParamValue::String(_) => "string",
            ParamValue::Enum(_, _) => "enum",
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            ParamValue::Bool(v) => format!("{}", v),
            ParamValue::Int(v) => format!("{}", v),
            ParamValue::Uint(v) => format!("{}", v),
            ParamValue::String(v) => v.clone(),
            ParamValue::Enum(v, _) => v.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamDef {
    pub name: String,
    pub description: String,
    pub default: ParamValue,
    pub writable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverParams {
    pub params: BTreeMap<String, ParamDef>,
    pub values: BTreeMap<String, ParamValue>,
}

impl DriverParams {
    pub fn new() -> Self {
        DriverParams {
            params: BTreeMap::new(),
            values: BTreeMap::new(),
        }
    }

    pub fn define(&mut self, name: &str, description: &str, default: ParamValue, writable: bool) {
        self.params.insert(
            String::from(name),
            ParamDef {
                name: String::from(name),
                description: String::from(description),
                default: default.clone(),
                writable,
            },
        );
        self.values.insert(String::from(name), default);
    }

    pub fn get(&self, name: &str) -> Option<&ParamValue> {
        self.values.get(name)
    }

    pub fn set(&mut self, name: &str, value: ParamValue) -> Result<(), &'static str> {
        match self.params.get(name) {
            Some(def) if !def.writable => Err("parameter is read-only"),
            Some(def) => {
                if core::mem::discriminant(&def.default) == core::mem::discriminant(&value) {
                    self.values.insert(String::from(name), value);
                    Ok(())
                } else {
                    Err("parameter type mismatch")
                }
            }
            None => Err("unknown parameter"),
        }
    }

    pub fn list(&self) -> Vec<&ParamDef> {
        self.params.values().collect()
    }

    pub fn parse_bool(s: &str) -> Option<bool> {
        match s.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        }
    }

    pub fn parse_int(s: &str) -> Option<i64> {
        s.parse().ok()
    }

    pub fn parse_uint(s: &str) -> Option<u64> {
        s.parse().ok()
    }
}

impl Default for DriverParams {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_and_get_parameter() {
        let mut p = DriverParams::new();
        p.define("debug", "Enable debug logging", ParamValue::Bool(false), true);
        assert_eq!(p.get("debug"), Some(&ParamValue::Bool(false)));
    }

    #[test]
    fn set_writable_parameter() {
        let mut p = DriverParams::new();
        p.define("debug", "Enable debug logging", ParamValue::Bool(false), true);
        assert!(p.set("debug", ParamValue::Bool(true)).is_ok());
        assert_eq!(p.get("debug"), Some(&ParamValue::Bool(true)));
    }

    #[test]
    fn set_readonly_parameter_fails() {
        let mut p = DriverParams::new();
        p.define("vendor_id", "Vendor ID", ParamValue::Uint(0), false);
        assert!(p.set("vendor_id", ParamValue::Uint(1)).is_err());
    }

    #[test]
    fn set_unknown_parameter_fails() {
        let mut p = DriverParams::new();
        assert!(p.set("nonexistent", ParamValue::Bool(true)).is_err());
    }

    #[test]
    fn param_value_display_strings() {
        assert_eq!(ParamValue::Bool(true).to_display_string(), "true");
        assert_eq!(ParamValue::Int(-42).to_display_string(), "-42");
        assert_eq!(ParamValue::Uint(42).to_display_string(), "42");
        assert_eq!(ParamValue::String(String::from("hello")).to_display_string(), "hello");
    }

    #[test]
    fn parse_bool_variants() {
        assert_eq!(DriverParams::parse_bool("true"), Some(true));
        assert_eq!(DriverParams::parse_bool("1"), Some(true));
        assert_eq!(DriverParams::parse_bool("yes"), Some(true));
        assert_eq!(DriverParams::parse_bool("false"), Some(false));
        assert_eq!(DriverParams::parse_bool("0"), Some(false));
    }
}
