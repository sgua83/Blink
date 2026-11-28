module bootstrap.common_facts;
import bootstrap.scalar_fold;

struct CommonFactInputs { address environment; array<address,2> sides; array<address,2> roots; };

extern "C" void common_facts(CommonFactInputs input,address output) on cpu;
extern "C" KnownProjection known_fact_values(address sets,u64 count,address identity,address scalar) on cpu;
