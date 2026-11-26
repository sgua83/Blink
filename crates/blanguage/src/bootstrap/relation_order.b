module bootstrap.relation_order;

const u64 REACHABLE = 1u64;
const u64 VARIABLE = 2u64;
const u64 READS = 4u64;

struct Dependencies {
    usize first;
    usize end;
    u64 flags;
};

struct State {
    usize pending;
    usize users_start;
    usize users_count;
    u64 changes;
};

struct Ordered {
    usize id;
    u64 changes;
};

address cell(address data, usize index, u64 stride) on cpu {
    return unsafe_address(address_bits(data) + u64(index) * stride);
}

Dependencies input_row(Input input, usize index) on cpu {
    return unsafe_position<Dependencies>(cell(input.rows, index, 24u64));
}

bool ordinary(Dependencies row) on cpu {
    return (row.flags & (REACHABLE | VARIABLE)) == REACHABLE;
}

usize dependency(Input input, usize index) on cpu {
    return unsafe_position<usize>(cell(input.dependencies, index, 8u64));
}

State state(Workspace work, usize index) on cpu {
    return unsafe_position<State>(cell(work.states, index, 32u64));
}

void store_state(Workspace work, usize index, State row) on cpu {
    unsafe_position<State>(cell(work.states, index, 32u64)) = row;
}

void initialize(Input input, Workspace work) on cpu {
    usize index = 0u64;
    while (index < input.count) {
        usize id = index;
        index += 1u64;
        Dependencies source = input_row(input, id);
        if (!ordinary(source)) { continue; }
        usize pending = 0u64;
        u64 changes = (source.flags & READS) == 0u64 ? 0u64 : 1u64;
        usize edge = source.first;
        while (edge < source.end) {
            usize producer = dependency(input, edge);
            edge += 1u64;
            if ((input_row(input, producer).flags & VARIABLE) != 0u64) {
                changes = 1u64;
            } else {
                pending += 1u64;
                State upstream = state(work, producer);
                upstream.users_count += 1u64;
                store_state(work, producer, upstream);
            }
        }
        State row = state(work, id);
        row.pending = pending;
        row.changes = changes;
        store_state(work, id, row);
    }

    usize start = 0u64;
    index = 0u64;
    while (index < input.count) {
        State row = state(work, index);
        row.users_start = start;
        start += row.users_count;
        row.users_count = 0u64;
        store_state(work, index, row);
        index += 1u64;
    }
}

usize connect(Input input, Workspace work) on cpu {
    usize ready = 0u64;
    usize index = 0u64;
    while (index < input.count) {
        usize id = index;
        index += 1u64;
        Dependencies source = input_row(input, id);
        if (!ordinary(source)) { continue; }
        if (state(work, id).pending == 0u64) {
            unsafe_position<usize>(cell(work.ready, ready, 8u64)) = id;
            ready += 1u64;
        }
        usize edge = source.first;
        while (edge < source.end) {
            usize producer = dependency(input, edge);
            edge += 1u64;
            if ((input_row(input, producer).flags & VARIABLE) == 0u64) {
                State upstream = state(work, producer);
                unsafe_position<usize>(cell(work.users, upstream.users_start + upstream.users_count, 8u64)) = id;
                upstream.users_count += 1u64;
                store_state(work, producer, upstream);
            }
        }
    }
    return ready;
}

void choose_fixed(Input input, Workspace work, address output) on cpu {
    usize index = 0u64;
    while (index < input.count) {
        usize id = index;
        index += 1u64;
        Dependencies source = input_row(input, id);
        if ((source.flags & (REACHABLE | VARIABLE | READS)) != (REACHABLE | VARIABLE)) { continue; }
        usize edge = source.first;
        while (edge < source.end) {
            usize producer = dependency(input, edge);
            State prerequisite = state(work, producer);
            if (!ordinary(input_row(input, producer)) || prerequisite.pending != 0u64 || prerequisite.changes != 0u64) {
                break;
            }
            edge += 1u64;
        }
        if (edge == source.end) {
            unsafe_position<u8>(cell(output, id, 1u64)) = u8(1);
        }
    }
}

extern "C" usize plan(Input input, Workspace work, Output output) on cpu {
    initialize(input, work);
    usize ready = connect(input, work);
    usize completed = 0u64;
    bool materialize = address_bits(output.ordered) != 0u64;
    while (ready != 0u64) {
        ready -= 1u64;
        usize id = unsafe_position<usize>(cell(work.ready, ready, 8u64));
        State row = state(work, id);
        if (materialize) {
            unsafe_position<Ordered>(cell(output.ordered, completed, 16u64)) = Ordered(id, row.changes);
        }
        completed += 1u64;
        usize item = 0u64;
        while (item < row.users_count) {
            usize user = unsafe_position<usize>(cell(work.users, row.users_start + item, 8u64));
            item += 1u64;
            State next = state(work, user);
            next.changes = next.changes | row.changes;
            next.pending -= 1u64;
            store_state(work, user, next);
            if (next.pending == 0u64) {
                unsafe_position<usize>(cell(work.ready, ready, 8u64)) = user;
                ready += 1u64;
            }
        }
    }
    if (materialize) {
        choose_fixed(input, work, output.fixed_inputs);
    }
    return completed;
}

struct CallNode {
    usize index;
    usize low;
};

struct CallVisit {
    usize node;
    usize next;
    u64 self_call;
};

CallRange call_row(CallGraph graph, usize node) on cpu {
    return unsafe_position<CallRange>(cell(graph.rows, node, size_of<CallRange>()));
}

CallNode call_state(CallWork work, usize node) on cpu {
    return unsafe_position<CallNode>(cell(work.states, node, size_of<CallNode>()));
}

void set_call_state(CallWork work, usize node, CallNode state) on cpu {
    unsafe_position<CallNode>(cell(work.states, node, size_of<CallNode>())) = state;
}

void enter_call(CallGraph graph, CallWork work, usize node, usize index,
                usize depth, usize active) on cpu {
    set_call_state(work, node, CallNode(index, index));
    unsafe_position<usize>(cell(work.active, active, 8u64)) = node;
    unsafe_position<CallVisit>(cell(work.visits, depth, size_of<CallVisit>())) =
        CallVisit(node, call_row(graph, node).first, 0u64);
}

extern "C" void recursive_call_groups(CallGraph graph, CallWork work, address groups) on cpu {
    // An index above the last possible discovery is a completed vertex.
    usize closed = graph.count + 1u64;
    usize discovered = 0u64;
    usize active = 0u64;
    usize root = 0u64;
    while (root < graph.count) {
        usize first = root;
        root += 1u64;
        if (call_state(work, first).index != 0u64) { continue; }

        discovered += 1u64;
        enter_call(graph, work, first, discovered, 0u64, active);
        active += 1u64;
        usize depth = 1u64;
        while (depth != 0u64) {
            address frame = cell(work.visits, depth - 1u64, size_of<CallVisit>());
            CallVisit visit = unsafe_position<CallVisit>(frame);
            CallNode node = call_state(work, visit.node);
            if (visit.next != call_row(graph, visit.node).end) {
                usize callee = unsafe_position<usize>(cell(graph.callees, visit.next, 8u64));
                visit.next += 1u64;
                if (callee == visit.node) { visit.self_call = 1u64; }
                unsafe_position<CallVisit>(frame) = visit;

                usize index = call_state(work, callee).index;
                if (index == 0u64) {
                    discovered += 1u64;
                    enter_call(graph, work, callee, discovered, depth, active);
                    active += 1u64;
                    depth += 1u64;
                } else if (index < node.low) {
                    set_call_state(work, visit.node, CallNode(node.index, index));
                }
                continue;
            }

            depth -= 1u64;
            if (node.low == node.index) {
                usize member = unsafe_position<usize>(cell(work.active, active - 1u64, 8u64));
                usize group = member == visit.node && visit.self_call == 0u64 ? 0u64 : node.index;
                while (true) {
                    active -= 1u64;
                    member = unsafe_position<usize>(cell(work.active, active, 8u64));
                    unsafe_position<usize>(cell(groups, member, 8u64)) = group;
                    set_call_state(work, member, CallNode(closed, closed));
                    if (member == visit.node) { break; }
                }
            }
            if (depth != 0u64) {
                CallVisit parent = unsafe_position<CallVisit>(cell(work.visits, depth - 1u64, size_of<CallVisit>()));
                CallNode upper = call_state(work, parent.node);
                if (node.low < upper.low) {
                    set_call_state(work, parent.node, CallNode(upper.index, node.low));
                }
            }
        }
    }
}
