fn main() -> blcompute::Result<()> {
    // Device policy belongs to this application. Change to "cpu" for CPU execution.
    blink_rust_examples::scene::run("gpu")
}
