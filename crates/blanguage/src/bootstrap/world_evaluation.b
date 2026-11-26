module bootstrap.world_evaluation;

extern "C" void enter(address top, address frame) on cpu {
    unsafe_position<address>(frame)=unsafe_position<address>(top);
    unsafe_position<address>(top)=frame;
}

address take(address top) on cpu {
    address frame=unsafe_position<address>(top);
    unsafe_position<address>(top)=unsafe_position<address>(frame);
    return frame;
}

extern "C" void drive(address top, address context,
    cfn<u32,address,address> poll, cfn<void,address> release) on cpu
    requires nonzero(poll) requires nonzero(release) {
    address frame=unsafe_position<address>(top);
    while (address_bits(frame)!=0u64) {
        // A pending invocation may have entered a child. Follow the current top.
        if (poll(frame,context)!=0) { release(take(top)); }
        frame=unsafe_position<address>(top);
    }
}

extern "C" void drain(address top, cfn<void,address> release) on cpu
    requires nonzero(release) {
    while (address_bits(unsafe_position<address>(top))!=0u64) {
        release(take(top));
    }
}
