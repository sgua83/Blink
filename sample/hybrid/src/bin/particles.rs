fn main() -> blcompute::Result<()> {
    let mut args = std::env::args().skip(1);
    let backend = args.next().ok_or("用法: particles cpu|gpu <数量>")?;
    let count = args.next().ok_or("缺少粒子数量")?.parse()?;
    blink_rust_examples::particles::run(&backend, count)
}
