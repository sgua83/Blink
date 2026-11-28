module bootstrap.proof_domain_join;

struct ProofDomainOperand {
    address domains;
    address origins;
    usize origin_count;
    address ranges;
    usize range_count;
};

extern "C" u32 join_proof_domain_sets(address output,ProofDomainOperand left,ProofDomainOperand right) on cpu;
