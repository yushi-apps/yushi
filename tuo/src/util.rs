use std::hash::{Hash, Hasher};


#[derive(Debug, Clone, PartialEq)]
pub struct SourceLocation {
    //path: String,
    //// 从1开始计数
    //line: i32,
    //// 从1开始计数
    //col: i32,
    //len: i32,
    //pos: i32
}

#[derive(Debug, Clone, PartialEq)]
pub enum YValue {
    None,
    String(String),
    Number(f64),
    Boolean(bool),
    Complex(serde_json::Value)
}

impl YValue {
    pub fn from_json(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => YValue::None,
            serde_json::Value::String(s) => YValue::String(s),
            serde_json::Value::Number(n) => YValue::Number(n.as_f64().unwrap_or(f64::NAN)),
            serde_json::Value::Bool(b) => YValue::Boolean(b),
            _ => YValue::Complex(value),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            YValue::None => serde_json::Value::Null,
            YValue::String(s) => serde_json::Value::String(s.clone()),
            YValue::Number(n) => serde_json::Value::Number(serde_json::Number::from_f64(*n).unwrap()),
            YValue::Boolean(b) => serde_json::Value::Bool(*b),
           YValue::Complex(v) => v.clone(),
        }
    }

    pub fn to_falsy(&self) -> bool {
        match self {
            YValue::None => true,
            YValue::String(s) => s.is_empty(),
            YValue::Number(n) => n.is_nan() || *n == 0.0,
            YValue::Boolean(b) => !*b,
            YValue::Complex(v) => v.is_null(),
        }
    }

    pub fn to_truthy(&self) -> bool {
        !self.to_falsy()
    }

    pub fn to_string(&self) -> String {
        match self {
            YValue::None => "None".to_string(),
            YValue::String(s) => s.clone(),
            YValue::Number(n) => n.to_string(),
            YValue::Boolean(b) => b.to_string(),
            YValue::Complex(v) => v.to_string(),
        }
    }
    //pub fn to_string(&self) -> String {
    //    let type_name = std::any::type_name::<Self>();
    //    let variant_str = match self {
    //        YValue::None => "None",
    //        YValue::String(_) => "String",
    //        YValue::Number(_) => "Number",
    //        YValue::Boolean(_) => "Boolean",
    //        YValue::Complex(_) => "Complex",
    //    };

    //    let mut hasher = DefaultHasher::new();
    //    self.hash(&mut hasher);
    //    let hash_code = hasher.finish(); 

    //    format!("{}::{}@{:x}", variant_str, type_name, hash_code)
    //}
}

impl Hash for YValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            YValue::None => {}
            YValue::String(s) => s.hash(state),
            YValue::Number(n) => n.to_bits().hash(state),
            YValue::Boolean(b) => b.hash(state),
            YValue::Complex(v) => v.hash(state),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValueWithLocation {
    pub value: YValue,
    pub location: Option<SourceLocation>,
}

impl ValueWithLocation {
    pub fn new(value: YValue, location: Option<SourceLocation>) -> Self {
        Self { 
            value, 
            location
        }
    }

    pub fn as_string(&self) -> Option<String> {
        if let YValue::String(s) = &self.value {
            Some(s.clone())
        } else {
            None
        }
    }

    pub fn location(&self) -> Option<SourceLocation> {
        self.location.clone()
    }

    pub fn value(&self) -> YValue {
        self.value.clone()
    }
}
