mod delete_instance;
mod instance;
mod page;

pub use delete_instance::DeleteInstanceModal;
pub use instance::{
    ChangeVersionModal, CreateInstanceModal, DuplicateInstanceModal, EditInstanceModal,
    GroupInstanceModal, InstanceFacts,
};
pub use page::Clusters;
