//! xdsl - 差量运算模块
//! 
//! 提供XML节点的合并算子和差量计算功能

mod merge;
mod diff;

pub use merge::{OverrideRule, YNodeMerge};
pub use diff::{YDelta, YModification, diff};
