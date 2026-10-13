//! 存储层：单文件容器与仓储。

pub mod container;
pub mod frame;
pub mod repository;

pub use container::Container;
pub use frame::Frame;
pub use repository::{FolderDeleteStrategy, ItemDraft, Repository};
