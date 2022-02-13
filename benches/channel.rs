use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures_executor::block_on;
use futures_signals::signal::{channel, SignalExt};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("channel", |b| {
        b.iter(|| {
            let (sender, receiver) = channel(black_box(0));

            let handle = std::thread::spawn(move || {
                block_on(receiver.for_each(|value| {
                    assert!(value >= 0);
                    async move {}
                }));
            });

            let _ = sender.send(black_box(1));

            drop(sender);

            let _ = handle.join();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
