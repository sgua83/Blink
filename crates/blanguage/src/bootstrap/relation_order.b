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

address cell(address data, usize index, u64 stride) on cpu {
    return unsafe_address(address_bits(data) + u64(index) * stride);
}


const usize WALK_END = 18446744073709551615u64;
const u32 WALK_SEEN = 0;
const u32 WALK_MATCHED = 2;

void walk_frames(address graph,address work,WalkSlice edges,u32 frame_read,u32 sources,WalkEdges host) on cpu
    requires nonzero(host.source.edge) requires nonzero(host.source.frame) requires nonzero(host.source.write)
    requires nonzero(host.source.field) requires nonzero(host.mark_frame) requires nonzero(host.publish)
{
    usize index = 0u64;
    while (index < edges.count) {
        usize frame = host.source.edge(edges.data,index).frame;
        index += 1u64;
        while (frame != WALK_END && host.mark_frame(work,WalkVisit(frame,frame_read)) != 0) {
            WalkFrame current = host.source.frame(graph,frame);
            frame = current.parent;
            if (sources == 0) { continue; }
            usize item = 0u64;
            while (item < current.writes.count) {
                WalkWrite write = host.source.write(current.writes.data,item);
                item += 1u64;
                host.publish(work,WalkVisit(write.target,frame_read));
                usize field = 0u64;
                while (field < write.fields.count) {
                    host.publish(work,WalkVisit(host.source.field(write.fields.data,field),frame_read));
                    field += 1u64;
                }
            }
        }
    }
}

void walk_inputs(address graph,address work,WalkVisit visit,u32 feedback,WalkEdges host) on cpu
    requires nonzero(host.source.edges) requires nonzero(host.source.edge)
    requires nonzero(host.source.frame) requires nonzero(host.source.write)
    requires nonzero(host.source.field) requires nonzero(host.mark_frame) requires nonzero(host.publish)
{
    WalkSlice edges = host.source.edges(graph,visit.node);
    u32 frame_read = visit.read;
    usize index = 0u64;
    while (index < edges.count) {
        WalkEdge edge = host.source.edge(edges.data,index);
        u32 read = visit.read | (feedback & edge.read);
        frame_read |= read;
        host.publish(work,WalkVisit(edge.source,read));
        index += 1u64;
    }

    walk_frames(graph,work,edges,frame_read,1,host);
}

extern "C" u32 walk_relation(address graph,address work,usize root,u32 feedback,WalkHost host) on cpu
    requires nonzero(host.edges.source.edges) requires nonzero(host.edges.source.edge)
    requires nonzero(host.edges.source.frame) requires nonzero(host.edges.source.write)
    requires nonzero(host.edges.source.field) requires nonzero(host.edges.mark_frame)
    requires nonzero(host.edges.publish) requires nonzero(host.pop) requires nonzero(host.visit)
{
    host.edges.publish(work,WalkVisit(root,0));
    while (true) {
        WalkVisit visit = host.pop(work);
        if (visit.node == WALK_END) { return 0; }
        if (visit.node == root && visit.read != 0) { return 1; }
        u32 discovered = host.visit(work,visit);
        if (discovered == WALK_MATCHED) { return 1; }
        if (discovered == WALK_SEEN) { continue; }
        walk_inputs(graph,work,visit,feedback,host.edges);
    }
}

extern "C" void relation_dependencies(address graph,address work,usize root,WalkEdges host) on cpu
    requires nonzero(host.source.edges) requires nonzero(host.source.edge)
    requires nonzero(host.source.frame) requires nonzero(host.source.write)
    requires nonzero(host.source.field) requires nonzero(host.mark_frame) requires nonzero(host.publish)
{
    walk_inputs(graph,work,WalkVisit(root,0),0,host);
}

extern "C" void historical_relation_frames(address graph,address work,address roots,cfn<usize,address> next,WalkEdges host) on cpu
    requires nonzero(next) requires nonzero(host.source.edges) requires nonzero(host.source.edge)
    requires nonzero(host.source.frame) requires nonzero(host.source.write) requires nonzero(host.source.field)
    requires nonzero(host.mark_frame) requires nonzero(host.publish)
{
    usize root = next(roots);
    while (root != WALK_END) {
        walk_frames(graph,work,host.source.edges(graph,root),0,0,host);
        root = next(roots);
    }
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

void finish_order(Output output, usize stable, usize feedback) on cpu {
    usize count = output.ordered_count - feedback;
    usize offset = 0u64;
    while (offset < count / 2u64) {
        address left = cell(output.ordered, feedback + offset, 8u64);
        address right = cell(output.ordered, output.ordered_count - 1u64 - offset, 8u64);
        usize first = unsafe_position<usize>(left);
        unsafe_position<usize>(left) = unsafe_position<usize>(right);
        unsafe_position<usize>(right) = first;
        offset += 1u64;
    }
    // A partially ordered graph leaves a gap between the two written ranges.
    if (stable != feedback) {
        offset = 0u64;
        while (offset < count) {
            unsafe_position<usize>(cell(output.ordered, stable + offset, 8u64)) =
                unsafe_position<usize>(cell(output.ordered, feedback + offset, 8u64));
            offset += 1u64;
        }
    }
}

extern "C" OrderCount plan(Input input, Workspace work, Output output) on cpu {
    initialize(input, work);
    usize ready = connect(input, work);
    usize completed = 0u64;
    usize stable = 0u64;
    usize feedback = output.ordered_count;
    bool materialize = address_bits(output.ordered) != 0u64;
    while (ready != 0u64) {
        ready -= 1u64;
        usize id = unsafe_position<usize>(cell(work.ready, ready, 8u64));
        State row = state(work, id);
        if (materialize) {
            if (row.changes == 0u64) {
                unsafe_position<usize>(cell(output.ordered, stable, 8u64)) = id;
                stable += 1u64;
            } else {
                feedback -= 1u64;
                unsafe_position<usize>(cell(output.ordered, feedback, 8u64)) = id;
            }
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
        finish_order(output, stable, feedback);
        choose_fixed(input, work, output.fixed_inputs);
    }
    return OrderCount(completed, stable);
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

bool returns_unchanged(address graph,usize variable,FeedbackEdge first,IdentityFeedbackHost host) on cpu
    requires nonzero(host.edge) requires nonzero(host.node)
{
    FeedbackEdge edge = first;
    while (edge.identity != 0) {
        if (edge.from == variable) { return true; }
        FeedbackNode node = host.node(graph,edge.from);
        if (node.variable != 0 || node.count != 1u64 || node.seeded != 0) { return false; }
        edge = host.edge(node.edges,0u64);
    }
    return false;
}

extern "C" usize prune_identity_feedback(address graph,address edges,usize count,usize variable,IdentityFeedbackHost host) on cpu
    requires nonzero(host.edge) requires nonzero(host.node) requires nonzero(host.exchange)
{
    usize index = 0u64;
    usize retained = 0u64;
    while (index < count) {
        FeedbackEdge edge = host.edge(edges,index);
        if (!returns_unchanged(graph,variable,edge,host)) {
            if (retained != index) { host.exchange(edges,retained,index); }
            retained += 1u64;
        }
        index += 1u64;
    }
    return retained;
}

bool round_unchanged(address context, usize count, u32 paths, RoundHost host) on cpu
    requires nonzero(host.equivalent)
{
    usize index = 0u64;
    while (index < count) {
        if (host.equivalent(context,index,paths) == 0) { return false; }
        index += 1u64;
    }
    return true;
}

extern "C" u32 solve_round(address context, RoundSchedule schedule, u32 initial, RoundHost host) on cpu
    requires nonzero(host.evaluate) requires nonzero(host.next_variable) requires nonzero(host.observe)
    requires nonzero(host.equivalent) requires nonzero(host.forget)
{
    usize index = initial != 0 ? 0u64 : schedule.stable;
    while (index < schedule.count) {
        usize id = unsafe_position<usize>(cell(schedule.ordered,index,8u64));
        host.evaluate(context,id,0);
        index += 1u64;
    }

    usize active = 0u64;
    usize variable = host.next_variable(context);
    while (variable != WALK_END) {
        if (initial != 0 || unsafe_position<u8>(cell(schedule.fixed_inputs,variable,1u64)) == u8(0)) {
            host.evaluate(context,variable,1);
            active += 1u64;
        }
        variable = host.next_variable(context);
    }

    host.observe(context);
    if (round_unchanged(context,active,0,host) && round_unchanged(context,active,1,host)) {
        return 1;
    }

    index = schedule.stable;
    while (index < schedule.count) {
        host.forget(context,unsafe_position<usize>(cell(schedule.ordered,index,8u64)));
        index += 1u64;
    }
    return 0;
}

usize old_id(WalkSlice ids, usize index) on cpu {
    return unsafe_position<usize>(cell(ids.data,index,8u64));
}

usize rank_of(WalkSlice ids, usize id) on cpu {
    usize first = 0u64;
    usize end = ids.count;
    while (first < end) {
        usize middle = first + (end - first) / 2u64;
        if (old_id(ids,middle) < id) {
            first = middle + 1u64;
        } else {
            end = middle;
        }
    }
    return first < ids.count && old_id(ids,first) == id ? first : WALK_END;
}

extern "C" usize compact_index(RelationSelection selection, usize id) on cpu {
    if (id == selection.root) { return 0u64; }
    usize rank = rank_of(selection.nodes,id);
    if (rank == WALK_END) { return rank; }
    return id < selection.root ? rank + 1u64 : rank;
}


struct RelationIndices {
    RelationSelection selection;
    RelationOffsets offsets;
    bool compacted;
};

usize relation_node_index(RelationIndices map, usize id) on cpu {
    return map.compacted ? compact_index(map.selection,id) : map.offsets.nodes + id;
}

usize relation_frame_index(RelationIndices map, usize id) on cpu {
    if (id == WALK_END) { return map.offsets.prefix; }
    return map.compacted ? rank_of(map.selection.frames,id) : map.offsets.frames + id;
}

void transfer_node(address graph, address destination, RelationIndices map, usize id,
    address entry, RelationSource source, RelationOutput output) on cpu
    requires nonzero(source.edges) requires nonzero(source.edge)
    requires nonzero(output.node) requires nonzero(output.edge)
{
    WalkSlice edges = source.edges(graph,id);
    address node = output.node(destination,id,relation_node_index(map,id),edges.count);
    usize index = 0u64;
    while (index < edges.count) {
        WalkEdge edge = source.edge(edges.data,index);
        output.edge(node,edges.data,index,relation_node_index(map,edge.source),relation_frame_index(map,edge.frame),entry);
        index += 1u64;
    }
}

void transfer_frame(address graph, address destination, RelationIndices map, usize id, RelationSource source, RelationOutput output) on cpu
    requires nonzero(source.frame) requires nonzero(source.write) requires nonzero(source.field)
    requires nonzero(output.frame) requires nonzero(output.write) requires nonzero(output.field)
{
    WalkFrame frame = source.frame(graph,id);
    address next_frame = output.frame(destination,relation_frame_index(map,frame.parent),frame.writes.count);
    usize item = 0u64;
    while (item < frame.writes.count) {
        WalkWrite write = source.write(frame.writes.data,item);
        address fields = output.write(next_frame,frame.writes.data,item,relation_node_index(map,write.target));
        usize field = 0u64;
        while (field < write.fields.count) {
            usize target = source.field(write.fields.data,field);
            output.field(fields,write.fields.data,field,relation_node_index(map,target));
            field += 1u64;
        }
        item += 1u64;
    }
}

extern "C" void compact(address graph, address destination, RelationSelection selection, RelationSource source, RelationOutput output) on cpu
    requires nonzero(source.edges) requires nonzero(source.edge)
    requires nonzero(source.frame) requires nonzero(source.write) requires nonzero(source.field)
    requires nonzero(output.node) requires nonzero(output.edge)
    requires nonzero(output.frame) requires nonzero(output.write) requires nonzero(output.field)
{
    RelationIndices map = RelationIndices(selection,RelationOffsets(0u64,0u64,WALK_END),true);
    address entry = unsafe_address(0u64);
    transfer_node(graph,destination,map,selection.root,entry,source,output);
    usize index = 0u64;
    while (index < selection.nodes.count) {
        usize id = old_id(selection.nodes,index);
        if (id != selection.root) { transfer_node(graph,destination,map,id,entry,source,output); }
        index += 1u64;
    }

    index = 0u64;
    while (index < selection.frames.count) {
        transfer_frame(graph,destination,map,old_id(selection.frames,index),source,output);
        index += 1u64;
    }
}

extern "C" usize import_relation(address graph, address destination, usize root, usize nodes, usize frames,
    RelationOffsets offsets, address entry, RelationSource source, RelationOutput output, cfn<void,address,usize> bind_inputs) on cpu
    requires nonzero(source.edges) requires nonzero(source.edge)
    requires nonzero(source.frame) requires nonzero(source.write) requires nonzero(source.field)
    requires nonzero(output.node) requires nonzero(output.edge)
    requires nonzero(output.frame) requires nonzero(output.write) requires nonzero(output.field)
    requires nonzero(bind_inputs)
{
    WalkSlice empty = WalkSlice(unsafe_address(0u64),0u64);
    RelationIndices map = RelationIndices(RelationSelection(empty,empty,0u64),offsets,false);
    usize index = 0u64;
    while (index < frames) {
        transfer_frame(graph,destination,map,index,source,output);
        index += 1u64;
    }
    if (address_bits(entry) == 0u64) { bind_inputs(destination,offsets.nodes); }
    index = 0u64;
    while (index < nodes) {
        transfer_node(graph,destination,map,index,entry,source,output);
        index += 1u64;
    }
    return relation_node_index(map,root);
}

const u32 RESOLVED_NODE = 1;
const u32 BOUND_NODE = 2;
const u32 VARIABLE_NODE = 4;
const u32 READ_NODE = 8;

extern "C" void project_return(address graph, address context, usize nodes, WalkSlice frames, RelationSource source, ResidualOutput output) on cpu
    requires nonzero(source.frame) requires nonzero(source.write) requires nonzero(source.field)
    requires nonzero(output.node_facts) requires nonzero(output.node) requires nonzero(output.keep)
    requires nonzero(output.write) requires nonzero(output.field)
{
    usize id = 0u64;
    while (id < nodes) {
        u32 facts = output.node_facts(context,id);
        bool solved = (facts & (RESOLVED_NODE | BOUND_NODE)) != 0;
        bool publishes = (facts & (VARIABLE_NODE | READ_NODE)) != 0;
        output.node(context,id,facts & RESOLVED_NODE,solved && publishes ? 1 : 0);
        id += 1u64;
    }

    usize index = 0u64;
    while (index < frames.count) {
        usize frame_id = old_id(frames,index);
        WalkFrame frame = source.frame(graph,frame_id);
        usize item = 0u64;
        while (item < frame.writes.count) {
            WalkWrite write = source.write(frame.writes.data,item);
            u32 keep = output.keep(context,write.target);
            output.write(context,frame_id,item,keep);
            if (keep != 0) {
                usize field = 0u64;
                while (field < write.fields.count) {
                    output.field(context,source.field(write.fields.data,field));
                    field += 1u64;
                }
            }
            item += 1u64;
        }
        index += 1u64;
    }
}

const u32 REPLAY_FINISH_FRAME = 1;
const u32 REPLAY_STOP = 2;

extern "C" u32 replay_writes(address graph, address context, usize frame,
    cfn<WalkFrame,address,usize> frame_at, cfn<u32,address,address,usize> write) on cpu
    requires nonzero(frame_at) requires nonzero(write)
{
    usize at = frame;
    while (at != WALK_END) {
        WalkFrame current = frame_at(graph,at);
        usize remaining = current.writes.count;
        bool next_frame = true;
        while (remaining != 0u64) {
            remaining -= 1u64;
            u32 step = write(context,current.writes.data,remaining);
            if (step == REPLAY_STOP) { return 0; }
            if (step == REPLAY_FINISH_FRAME) { next_frame = false; }
        }
        if (!next_frame) { return 0; }
        at = current.parent;
    }
    return 1;
}

PrefixBoundary prefix_boundary(WalkSlice runs, usize index) on cpu {
    return unsafe_position<PrefixBoundary>(cell(runs.data,index,16u64));
}

usize prefix_after(WalkSlice runs, usize memory) on cpu {
    usize first = 0u64;
    usize end = runs.count;
    while (first < end) {
        usize middle = first + (end - first) / 2u64;
        if (prefix_boundary(runs,middle).memory <= memory) {
            first = middle + 1u64;
        } else {
            end = middle;
        }
    }
    return first;
}

extern "C" usize lexical_frame(address context, usize memory, PrefixHost host) on cpu
    requires nonzero(host.state) requires nonzero(host.emit) requires nonzero(host.publish)
{
    PrefixState prefix = host.state(context);
    while (prefix.through < memory) {
        usize at = prefix.through;
        usize parent = prefix.runs.count == 0u64 ? WALK_END : prefix_boundary(prefix.runs,prefix.runs.count - 1u64).frame;
        usize frame = host.emit(context,at,parent);
        prefix = host.state(context);
        if (frame != WALK_END) {
            // An inner read can advance past the inherited slot before it is published.
            usize slot = at + 1u64;
            usize position = prefix_after(prefix.runs,slot);
            bool restore = slot < prefix.through
                && (position == prefix.runs.count || prefix_boundary(prefix.runs,position).memory != slot + 1u64);
            host.publish(context,position,PrefixBoundary(slot,frame),restore ? 1 : 0,parent);
            prefix = host.state(context);
        }
    }

    usize end = prefix_after(prefix.runs,memory);
    return end == 0u64 ? WALK_END : prefix_boundary(prefix.runs,end - 1u64).frame;
}

extern "C" void label_lexical_frames(WalkSlice runs, address frames, usize count) on cpu {
    usize remaining = count;
    usize index = 0u64;
    while (remaining != 0u64 && index < runs.count) {
        PrefixBoundary run = prefix_boundary(runs,index);
        address place = cell(frames,remaining - 1u64,16u64);
        PrefixBoundary frame = unsafe_position<PrefixBoundary>(place);
        if (run.frame == frame.frame) {
            unsafe_position<PrefixBoundary>(place) = PrefixBoundary(run.memory - 1u64,frame.frame);
            remaining -= 1u64;
        }
        index += 1u64;
    }
}

void bind_relation_items(address context, RelationBindSlice part, cfn<void,address,address,address,usize> emit) on cpu
    requires nonzero(emit)
{
    usize index = 0u64;
    while (index < part.source.count) {
        emit(context,part.destination,part.source.data,index);
        index += 1u64;
    }
}

extern "C" void bind_relation_values(address context, usize nodes, usize frames, RelationBindOutput output) on cpu
    requires nonzero(output.node) requires nonzero(output.edge)
    requires nonzero(output.frame) requires nonzero(output.write)
{
    usize index = 0u64;
    while (index < nodes) {
        RelationBindSlice node = output.node(context,index);
        bind_relation_items(context,node,output.edge);
        index += 1u64;
    }

    index = 0u64;
    while (index < frames) {
        RelationBindSlice frame = output.frame(context,index);
        bind_relation_items(context,frame,output.write);
        index += 1u64;
    }
}
