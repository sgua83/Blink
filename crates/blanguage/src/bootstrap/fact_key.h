module bootstrap.fact_key;

struct FactKey { array<u64,18> words; };

extern "C" i32 order_identity(address left, address right) on cpu;
extern "C" i32 order_domain(address left, address right) on cpu;
extern "C" i32 order_operand(address left, address right) on cpu;
extern "C" i32 order_selector(address left, address right) on cpu;
extern "C" i32 order_predicate(address left, address right) on cpu;
extern "C" i32 order_expression(address left, address right) on cpu;
extern "C" i32 order_floating(address left, address right) on cpu;
extern "C" i32 order_fact(address left, address right) on cpu;
