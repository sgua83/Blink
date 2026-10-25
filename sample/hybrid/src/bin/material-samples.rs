fn main() -> blcompute::Result<()> {
    let backend = std::env::args()
        .nth(1)
        .ok_or("用法: material-samples cpu|gpu")?;
    blink_rust_examples::sheet::run(&backend)
}
