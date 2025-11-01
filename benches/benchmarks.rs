//! # BEY 性能基准测试
//!
//! 使用 Criterion 进行性能基准测试

use criterion::{black_box, criterion_group, criterion_main, Criterion};

// 设备ID生成性能测试
fn bench_device_id_generation(c: &mut Criterion) {
    c.bench_function("device_id_generation", |b| {
        b.iter(|| {
            // 使用 black_box 防止编译器优化
            let _ = black_box(uuid::Uuid::new_v4().to_string());
        });
    });
}

// 创建基准测试组
criterion_group!(benches, bench_device_id_generation);

// 主入口
criterion_main!(benches);
