//! Background services and heavy processing (spec §16.2): image conversion
//! for attachments, the cleanup of unreferenced attachments and the sampling
//! of the notification queue.

pub mod cleanup;
pub mod images;
pub mod metrics;
