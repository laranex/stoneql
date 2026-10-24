use dbadmin_contracts::v1::{CellValue, typescript};
use serde_json::json;
use std::{hint::black_box, time::Instant};
fn main() {
    let fixture = include_str!("../tests/fixtures/values.json");
    let mut decode = Vec::new();
    let mut generate = Vec::new();
    for _ in 0..10 {
        black_box(serde_json::from_str::<Vec<CellValue>>(fixture).unwrap());
        black_box(typescript());
    }
    for _ in 0..1000 {
        let start = Instant::now();
        black_box(serde_json::from_str::<Vec<CellValue>>(black_box(fixture)).unwrap());
        decode.push(start.elapsed().as_nanos() as u64);
        let start = Instant::now();
        black_box(typescript());
        generate.push(start.elapsed().as_nanos() as u64);
    }
    println!(
        "{}",
        json!({"profile":"debug microbenchmark; not production SLO evidence","iterations":1000,"warmups":10,"fixture_bytes":fixture.len(),"decode_ns":decode,"generation_ns":generate})
    );
}
