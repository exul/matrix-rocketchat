use ruma_events::collections::all::Event;

/// A collection of Matrix events.
#[derive(Deserialize)]
pub struct Events {
    /// Matrix events
    pub events: Vec<Box<Event>>,
}
