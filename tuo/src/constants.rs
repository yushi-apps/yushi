/// Core constants for YNode processing
pub struct CoreConstants;

impl CoreConstants {
    /// Tag name for text nodes
    pub const TEXT_TAG_NAME: &'static str = "#text";
    
    /// Namespace prefix for xdsl
    pub const XDSL_NS_PREFIX: &'static str = "x";
    
    /// x:extends attribute name
    pub const X_EXTENDS: &'static str = "x:extends";
    
    /// x:override attribute name
    pub const X_OVERRIDE: &'static str = "x:override";
    
    /// x:override value: merge (default)
    pub const OVERRIDE_MERGE: &'static str = "merge";
    
    /// x:override value: append
    pub const OVERRIDE_APPEND: &'static str = "append";
    
    /// x:override value: prepend
    pub const OVERRIDE_PREPEND: &'static str = "prepend";
    
    /// x:override value: replace
    pub const OVERRIDE_REPLACE: &'static str = "replace";
    
    /// x:override value: delete
    pub const OVERRIDE_DELETE: &'static str = "delete";
}
