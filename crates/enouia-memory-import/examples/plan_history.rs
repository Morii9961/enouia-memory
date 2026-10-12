//! Built-in synthetic input only. No command line paths or model calls.
use enouia_memory_import::detect::{Detection, detect};
use enouia_memory_import::history::{HistoryLimits, plan_history};
use enouia_memory_import::zip::ZipLimits;

fn main() {
    let bytes = include_bytes!("../../../tests/fixtures/history/initialization-synthetic.json");
    let Detection::Parsed(input) = detect(bytes, &ZipLimits::default()) else {
        panic!("synthetic fixture must parse");
    };
    let plan = plan_history(
        &input,
        HistoryLimits {
            messages_per_batch: 2,
            ..HistoryLimits::default()
        },
    )
    .expect("synthetic plan");
    println!("SYNTHETIC ONLY: read-only planning, no extraction or approval");
    println!(
        "dated={} undated={} unparseable={} incomplete={} unavailable_attachments={}",
        plan.chronological.len(),
        plan.undated.len(),
        plan.unparseable,
        plan.incomplete_messages,
        plan.unavailable_attachments
    );
    println!("observed_range={:?}..{:?}", plan.earliest, plan.latest);
    for batch in &plan.batches {
        let unit = &input.units[batch.leaf.unit];
        let ids: Vec<_> = batch
            .messages
            .iter()
            .map(|r| unit.messages[r.message].upstream_id.as_deref())
            .collect();
        println!(
            "conversation={:?} leaf={:?} selected={} messages={ids:?} preceding={:?}",
            unit.conversation_id,
            unit.messages[batch.leaf.message].upstream_id,
            batch.selected_in_export,
            batch.preceding
        );
    }
    println!("issues={:?}", plan.issues);
}
