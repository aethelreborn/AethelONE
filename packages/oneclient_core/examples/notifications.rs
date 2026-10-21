// The import path nests its awaits deeply enough that the layout of the async
// block needs more room than the default recursion limit gives it
#![recursion_limit = "256"]

use oneclient_events::{EventBus, GroupedProgressSession};
use tokio::sync::mpsc;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let events = EventBus::new(tx);

    events.notify("Welcome").body("Launcher started").send();
    events
        .notify("Heads up")
        .body("Example error notification")
        .error()
        .send();

    let download = Uuid::new_v4();
    for current in (0..=100).step_by(25) {
        events.progress(download, "Downloading assets", current, 100);
    }

    let session = GroupedProgressSession::start(&events, "Preparing cluster".to_string());
    session.finish();

    drop(events);

    while let Some(notification) = rx.recv().await {
        println!("{notification:?}");
    }
}
