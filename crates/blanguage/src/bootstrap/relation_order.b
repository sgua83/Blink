module bootstrap.relation_order;
import bootstrap.clause_storage;

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

void walk_frames(address graph,address work,WalkSlice edges,u32 frame_read,WalkEdges host) on cpu
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

u32 walk_inputs(address graph,address work,WalkVisit visit,u32 feedback,WalkEdges host) on cpu
    requires nonzero(host.source.edges) requires nonzero(host.source.edge)
    requires nonzero(host.source.frame) requires nonzero(host.source.write)
    requires nonzero(host.source.field) requires nonzero(host.mark_frame) requires nonzero(host.publish)
{
    WalkSlice edges = host.source.edges(graph,visit.node);
    u32 reads = 0;
    usize index = 0u64;
    while (index < edges.count) {
        WalkEdge edge = host.source.edge(edges.data,index);
        u32 read = visit.read | (feedback & edge.read);
        reads |= edge.read;
        host.publish(work,WalkVisit(edge.source,read));
        index += 1u64;
    }

    walk_frames(graph,work,edges,visit.read | (feedback & reads),host);
    return reads;
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

extern "C" usize prepare_plan_inputs(address graph, address work, address roots, address rows,
    u32 materialize, PlanInputsHost host) on cpu
    requires nonzero(host.edges.source.edges) requires nonzero(host.edges.source.edge)
    requires nonzero(host.edges.source.frame) requires nonzero(host.edges.source.write)
    requires nonzero(host.edges.source.field) requires nonzero(host.edges.mark_frame)
    requires nonzero(host.edges.publish) requires nonzero(host.next) requires nonzero(host.variable)
    requires nonzero(host.begin) requires nonzero(host.finish)
{
    usize ordinary_count = 0u64;
    usize end = 0u64;
    usize id = host.next(roots);
    while (id != WALK_END) {
        u32 variable = host.variable(graph,id);
        if (variable == 0) { ordinary_count += 1u64; }
        usize first = end;
        u64 flags = REACHABLE | (variable != 0 ? VARIABLE : 0u64);
        if (materialize != 0 || variable == 0) {
            host.begin(work,id);
            u32 reads = walk_inputs(graph,work,WalkVisit(id,0),0,host.edges);
            if (materialize != 0 && reads != 0) { flags |= READS; }
            end = host.finish(work,first);
        }
        unsafe_position<Dependencies>(cell(rows,id,size_of<Dependencies>())) = Dependencies(first,end,flags);
        id = host.next(roots);
    }
    return ordinary_count;
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

const u8 FIXED_INPUT = u8(1);
const u8 DIRECT_READ = u8(2);

void prepare_schedule(Input input, Workspace work, Output output) on cpu {
    usize input_index = 0u64;
    usize index = 0u64;
    while (index < input.count) {
        usize id = index;
        index += 1u64;
        Dependencies source = input_row(input, id);
        u64 kind = source.flags & (REACHABLE | VARIABLE | READS);
        if (kind == (REACHABLE | READS)) {
            unsafe_position<u8>(cell(output.schedule_flags, id, 1u64)) = DIRECT_READ;
        }
        if ((kind & (REACHABLE | VARIABLE)) != (REACHABLE | VARIABLE)) { continue; }
        unsafe_position<usize>(cell(output.inputs, input_index, 8u64)) = id;
        input_index += 1u64;
        if ((kind & READS) != 0u64) { continue; }
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
            unsafe_position<u8>(cell(output.schedule_flags, id, 1u64)) = FIXED_INPUT;
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
        prepare_schedule(input, work, output);
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

usize follow_identity(address context, usize root, usize stop, cfn<usize,address,usize> next) on cpu
    requires nonzero(next)
{
    usize node = root;
    while (node != stop) {
        usize destination = next(context,node);
        if (destination == WALK_END) { break; }
        node = destination;
    }
    return node;
}

extern "C" usize resolve_identity_binding(address context, usize root, cfn<usize,address,usize> next) on cpu
    requires nonzero(next)
{
    return follow_identity(context,root,WALK_END,next);
}

extern "C" usize prune_identity_feedback(address graph,address edges,usize count,usize variable,IdentityFeedbackHost host) on cpu
    requires nonzero(host.edge) requires nonzero(host.next) requires nonzero(host.exchange)
{
    usize index = 0u64;
    usize retained = 0u64;
    while (index < count) {
        usize from = host.edge(edges,index);
        if (from == WALK_END || follow_identity(graph,from,variable,host.next) != variable) {
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
    requires nonzero(host.evaluate) requires nonzero(host.observe)
    requires nonzero(host.equivalent) requires nonzero(host.forget)
{
    usize index = initial != 0 ? 0u64 : schedule.stable;
    while (index < schedule.count) {
        usize id = unsafe_position<usize>(cell(schedule.ordered,index,8u64));
        host.evaluate(context,id,0);
        index += 1u64;
    }

    usize active = 0u64;
    index = 0u64;
    while (index < schedule.inputs.count) {
        usize variable = unsafe_position<usize>(cell(schedule.inputs.data,index,8u64));
        index += 1u64;
        if (initial != 0 || (unsafe_position<u8>(cell(schedule.schedule_flags,variable,1u64)) & FIXED_INPUT) == u8(0)) {
            host.evaluate(context,variable,1);
            active += 1u64;
        }
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

void mark_project_frames(address graph, usize id, address marks, RelationSource source) on cpu
    requires nonzero(source.edges) requires nonzero(source.edge) requires nonzero(source.frame)
{
    WalkSlice edges = source.edges(graph,id);
    usize index = 0u64;
    while (index < edges.count) {
        usize frame = source.edge(edges.data,index).frame;
        index += 1u64;
        while (frame != WALK_END) {
            address mark = cell(marks,frame,1u64);
            if (unsafe_position<u8>(mark) != u8(0)) { break; }
            unsafe_position<u8>(mark) = u8(1);
            frame = source.frame(graph,frame).parent;
        }
    }
}

extern "C" void project_return(address graph, address context, usize nodes, address frame_marks,
    usize frame_count, address schedule_flags, RelationSource source, ResidualOutput output) on cpu
    requires nonzero(source.edges) requires nonzero(source.edge) requires nonzero(source.frame)
    requires nonzero(source.write) requires nonzero(source.field)
    requires nonzero(output.node_facts) requires nonzero(output.node) requires nonzero(output.keep)
    requires nonzero(output.write) requires nonzero(output.field)
{
    usize id = 0u64;
    while (id < nodes) {
        u32 facts = output.node_facts(context,id);
        if (facts != 0) { mark_project_frames(graph,id,frame_marks,source); }
        u32 resolved = facts & RESOLVED_NODE;
        bool publishes = (facts & BOUND_NODE) != 0
            || (resolved != 0 && (unsafe_position<u8>(cell(schedule_flags,id,1u64)) & DIRECT_READ) != u8(0));
        output.node(context,id,resolved,publishes ? 1 : 0);
        id += 1u64;
    }

    usize index = 0u64;
    while (index < frame_count) {
        usize frame_id = index;
        index += 1u64;
        if (unsafe_position<u8>(cell(frame_marks,frame_id,1u64)) == u8(0)) { continue; }
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
    }
}

extern "C" u32 expand_initial_definition(address context, usize count, InitialDefinitionHost host) on cpu
    requires nonzero(host.initial) requires nonzero(host.begin) requires nonzero(host.edge)
{
    usize index = 0u64;
    while (index < count && host.initial(context,index) == 0) {
        index += 1u64;
    }
    if (index == count) { return 0; }
    host.begin(context,index);
    while (index < count) {
        host.edge(context,index);
        index += 1u64;
    }
    return 1;
}

extern "C" address select_read_path(address left, address right, u32 left_proven, cfn<u32,address> conflict,
    cfn<u32,address,u32> required) on cpu
    requires nonzero(conflict) requires nonzero(required)
{
    address selected = combine_clause_both(left,right,conflict);
    StoredClause condition = unsafe_position<StoredClause>(selected);
    // A target may contribute no facts beyond the write path already queried.
    if (condition.kind == StoredClauseKind::Facts || (left_proven != 0 && selected == left)) { return selected; }
    if (condition.kind != StoredClauseKind::Impossible
        && required(address_of(selected),u32(selected == left || selected == right)) != 0) {
        return selected;
    }
    clause_release(selected);
    return address();
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

const u32 WRITES_MEMORY = 1;
const u32 HAS_WRITTEN_VALUE = 2;
const u32 HAS_ADDRESS_FIELDS = 4;
const u32 CARRIED_STOP = 2;

extern "C" u32 visit_carried_writes(address function, address scopes, address consumer, CarriedWriteHost host) on cpu
    requires nonzero(host.next_scope) requires nonzero(host.history)
    requires nonzero(host.memory_facts) requires nonzero(host.consider)
{
    bool carried = false;
    usize scope = host.next_scope(scopes);
    while (scope != WALK_END) {
        CarriedScope history = host.history(function,scope);
        usize at = history.first;
        while (at < history.end) {
            u32 facts = host.memory_facts(function,at);
            if ((facts & WRITES_MEMORY) != 0 && (facts & (HAS_WRITTEN_VALUE | HAS_ADDRESS_FIELDS)) != HAS_WRITTEN_VALUE) {
                u32 found = host.consider(function,consumer,scope,at,history.backedge);
                if (found == CARRIED_STOP) { return 1; }
                carried = carried || found != 0;
            }
            at += 1u64;
        }
        scope = host.next_scope(scopes);
    }
    return carried ? 1 : 0;
}

// History registers describe earlier iterations, outside the physical register index range.
const usize HISTORY_REGISTER = 9223372036854775808u64;

void write_history_scope(address function, address output, usize scope, HistoryBindingsHost host) on cpu
    requires nonzero(host.registers) requires nonzero(host.insert)
{
    HistoryRegisters range = host.registers(function,scope);
    usize index = range.first;
    while (index < range.end) {
        host.insert(output,index,index | HISTORY_REGISTER);
        index += 1u64;
    }
}

extern "C" void write_history_bindings(address function, address output, address source, address active,
    usize prior, HistoryBindingsHost host) on cpu
    requires nonzero(host.next_scope) requires nonzero(host.registers) requires nonzero(host.insert)
{
    usize current = host.next_scope(active);
    usize scope = host.next_scope(source);
    while (scope != WALK_END) {
        while (current < scope) {
            current = host.next_scope(active);
        }
        if (scope != current) {
            write_history_scope(function,output,scope,host);
        }
        scope = host.next_scope(source);
    }
    if (prior != WALK_END) {
        write_history_scope(function,output,prior,host);
    }
}

usize first_memory_call(WalkSlice calls, usize memory,
    cfn<MemoryCallFacts,address,usize> facts) on cpu
    requires nonzero(facts)
{
    usize first = 0u64;
    usize end = calls.count;
    while (first < end) {
        usize middle = first + (end - first) / 2u64;
        if (facts(calls.data,middle).memory < memory) {
            first = middle + 1u64;
        } else {
            end = middle;
        }
    }
    return first;
}

extern "C" usize find_memory_call(WalkSlice calls, usize memory, u32 immediate,
    cfn<MemoryCallFacts,address,usize> facts) on cpu
    requires nonzero(facts)
{
    usize first = first_memory_call(calls,memory,facts);

    while (first < calls.count) {
        MemoryCallFacts call = facts(calls.data,first);
        if (call.memory != memory) { break; }
        if (immediate == 0 || call.deferred == 0) { return first; }
        first += 1u64;
    }
    return WALK_END;
}

const u32 NON_MEMORY_USE = 0;
const u32 CALL_MEMORY_USE = 2;

bool belongs_to_memory_call(MemoryUseFacts use, usize target) on cpu {
    return use.kind == CALL_MEMORY_USE && use.target == target;
}

extern "C" u32 any_call_memory_write(
    WalkSlice uses, usize first, u32 related, address test,
    cfn<MemoryUseFacts,address,usize> usage, cfn<u32,address,usize> overlaps
) on cpu requires nonzero(usage) requires nonzero(overlaps)
{
    if (overlaps(test,first) != 0) { return 1; }
    if (related == 0) { return 0; }
    usize index = first + 1u64;
    while (index < uses.count) {
        if (!belongs_to_memory_call(usage(uses.data,index),first)) { break; }
        if (overlaps(test,index) != 0) { return 1; }
        index += 1u64;
    }
    return 0;
}

extern "C" usize next_memory_event(WalkSlice uses, WalkSlice calls, address cursor, MemoryEventHost host) on cpu
    requires nonzero(host.usage) requires nonzero(host.call)
{
    MemoryEventCursor state = unsafe_position<MemoryEventCursor>(cursor);
    usize first = state.next_use;
    state.next_use += 1u64;
    MemoryUseFacts event = host.usage(uses.data,first);
    usize call = WALK_END;

    if (event.kind != NON_MEMORY_USE) {
        while (state.next_call < calls.count && state.call_memory == WALK_END) {
            MemoryCallFacts next = host.call(calls.data,state.next_call);
            if (next.deferred != 0) {
                state.next_call += 1u64;
            } else {
                state.call_memory = next.memory;
            }
        }
        if (state.call_memory == first) {
            call = state.next_call;
            state.next_call += 1u64;
            state.call_memory = WALK_END;
        } else if (event.kind == CALL_MEMORY_USE) {
            while (state.next_use < uses.count) {
                MemoryUseFacts next = host.usage(uses.data,state.next_use);
                if (!belongs_to_memory_call(next,event.target)) { break; }
                state.next_use += 1u64;
            }
        }
    }

    unsafe_position<MemoryEventCursor>(cursor) = state;
    return call;
}

extern "C" void visit_stored_locations(WalkSlice uses, WalkSlice calls, address context, StoredLocationHost host) on cpu
    requires nonzero(host.usage) requires nonzero(host.call) requires nonzero(host.project)
{
    usize call = 0u64;
    usize call_memory = WALK_END;
    usize memory = 0u64;
    while (memory < uses.count) {
        StoredUse event = host.usage(uses.data,memory);
        if (address_bits(event.path) != 0u64) {
            while (call < calls.count) {
                if (call_memory == WALK_END) {
                    call_memory = host.call(calls.data,call).memory;
                }
                if (call_memory >= memory) { break; }
                call += 1u64;
                call_memory = WALK_END;
            }
            if (call_memory == memory) {
                host.project(context,event,call);
            } else if (address_bits(event.direct) != 0u64) {
                host.project(context,event,WALK_END);
            }
        }
        memory += 1u64;
    }
}

extern "C" void visit_content_history(WalkSlice uses, WalkSlice calls, usize limit,
    address context, MemoryEventHost host, cfn<void,address,usize,usize> project) on cpu
    requires nonzero(host.usage) requires nonzero(host.call) requires nonzero(project)
{
    if (limit == 0u64) { return; }
    usize memory = limit;
    usize end = first_memory_call(calls,limit,host.call);
    MemoryCallFacts previous = MemoryCallFacts(WALK_END,0);
    while (memory != 0u64) {
        memory -= 1u64;
        if (host.usage(uses.data,memory).kind == NON_MEMORY_USE) { continue; }
        usize matched = WALK_END;
        while (end != 0u64) {
            if (previous.memory == WALK_END) {
                previous = host.call(calls.data,end - 1u64);
            }
            if (previous.memory < memory) { break; }
            end -= 1u64;
            if (previous.memory == memory && previous.deferred == 0) {
                matched = end;
            }
            previous.memory = WALK_END;
        }
        project(context,memory,matched);
    }
}

extern "C" void collect_stored_returns(WalkSlice uses, address context,
    cfn<u32,address,usize> marked, cfn<void,address,usize> accept) on cpu
    requires nonzero(marked) requires nonzero(accept)
{
    usize at = 0u64;
    while (at < uses.count) {
        if (marked(uses.data,at) != 0) { accept(context,at); }
        at += 1u64;
    }
}

extern "C" usize latest_multiple_stored_return(WalkSlice uses,
    cfn<u32,address,usize> marked) on cpu requires nonzero(marked)
{
    usize at = uses.count;
    usize latest = WALK_END;
    while (at != 0u64) {
        at -= 1u64;
        if (marked(uses.data,at) != 0) {
            if (latest != WALK_END) { return latest; }
            latest = at;
        }
    }
    return WALK_END;
}

ExitEvent exit_event_at(WalkSlice events, usize index) on cpu {
    return unsafe_position<ExitEvent>(cell(events.data,index,size_of<ExitEvent>()));
}

extern "C" void visit_exit_writes(WalkSlice events, usize memory, address context,
    cfn<void,address,usize,usize,usize> visit) on cpu requires nonzero(visit)
{
    usize first = 0u64;
    usize end = events.count;
    while (first < end) {
        usize middle = first + (end - first) / 2u64;
        if (exit_event_at(events,middle).at >= memory) {
            first = middle + 1u64;
        } else {
            end = middle;
        }
    }
    if (first == events.count) { return; }

    usize start = first == 0u64 ? 0u64 : exit_event_at(events,first - 1u64).end;
    while (first < events.count) {
        ExitEvent event = exit_event_at(events,first);
        visit(context,event.at,start,event.end);
        start = event.end;
        first += 1u64;
    }
}

extern "C" void collect_address_captures(usize count, u32 indexed, address context, CaptureHost host) on cpu
    requires nonzero(host.candidate) requires nonzero(host.same_index) requires nonzero(host.accept)
{
    usize at = 0u64;
    while (at < count) {
        CaptureIndex candidate = host.candidate(context,at);
        if (candidate == CaptureIndex::Whole || candidate == CaptureIndex::Element
            || (candidate == CaptureIndex::Concrete && indexed != 0 && host.same_index(context,at) != 0))
        {
            host.accept(context,at);
        }
        at += 1u64;
    }
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

void bind_relation_items(address context, usize parent, usize count, cfn<void,address,usize,usize> apply) on cpu
    requires nonzero(apply)
{
    usize index = 0u64;
    while (index < count) {
        apply(context,parent,index);
        index += 1u64;
    }
}

extern "C" void bind_relation_values(address context, usize nodes, usize frames, RelationValueOps ops) on cpu
    requires nonzero(ops.node) requires nonzero(ops.edge)
    requires nonzero(ops.frame) requires nonzero(ops.write)
{
    usize index = 0u64;
    while (index < nodes) {
        usize count = ops.node(context,index);
        bind_relation_items(context,index,count,ops.edge);
        index += 1u64;
    }

    index = 0u64;
    while (index < frames) {
        usize count = ops.frame(context,index);
        bind_relation_items(context,index,count,ops.write);
        index += 1u64;
    }
}

extern "C" void expand_component_ports(address context, cfn<usize,address> count,
    cfn<void,address,usize> visit) on cpu
    requires nonzero(count) requires nonzero(visit)
{
    usize port = 0u64;
    while (port < count(context)) {
        visit(context,port);
        port += 1u64;
    }
}

extern "C" void visit_relation_calls(address context, usize count, cfn<void,address,usize> visit) on cpu
    requires nonzero(visit)
{
    usize index = 0u64;
    while (index < count) {
        visit(context,index);
        index += 1u64;
    }
}
