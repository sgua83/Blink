module bootstrap.task_parameters;

extern "C" usize plan_input(address context,TypeRef input_type,usize input,usize argument,usize width,Host host) on cpu
    requires nonzero(host.visit) requires nonzero(host.length) {
    if (input_type.present != 0) {
        host.visit(context,input_type.value,Scope(input,argument,0u64,host.length(context),0));
    }
    return input + width;
}

extern "C" void plan_field(address context,Scope scope,usize component,address result,Host host) on cpu
    requires nonzero(host.length) requires nonzero(host.slot)
    requires nonzero(host.same_type) requires nonzero(host.append) requires nonzero(host.visit) {
    usize input = scope.nested == 0 ? scope.input + component : scope.input;
    Slot proposed = Slot(input,scope.argument,scope.parent,component,scope.nested);
    usize end = host.length(context);
    usize index = scope.first;
    while (index < end) {
        Slot existing = host.slot(context,index);
        if (existing.input == proposed.input && existing.nested == proposed.nested
            && (proposed.nested == 0 || (existing.parent == proposed.parent && existing.component == proposed.component))
            && host.same_type(context,index,result) != 0) {
            return;
        }
        index += 1u64;
    }

    usize parent = host.append(context,proposed,result);
    host.visit(context,result,Scope(input,scope.argument,parent,parent + 1u64,1));
}
