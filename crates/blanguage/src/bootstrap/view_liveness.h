module bootstrap.view_liveness;
import bootstrap.ast_storage;
import bootstrap.signatures;
struct Bytes {address data;u64 length;};
struct PathView {address path;u64 count;};
struct LocationView {Bytes name;PathView path;};
struct PathStep {Bytes field;u32 kind;u32 known;u32 index;};
struct Row {address key;address value;u64 count;u32 present;};
// Handles own the original native Live/Path containers. Iterators and rows only
// borrow those containers until their corresponding synchronous traversal ends.
struct Host {
    cfn<address> fresh;
    cfn<address,address> copy;
    cfn<void,address> release;
    cfn<bool,address,address> equal;
    cfn<address,Bytes> make_location;
    cfn<void,address,Bytes> field;
    cfn<void,address,Optional<u32>> index;
    cfn<void,address> drop_location;
    cfn<void,address,address> read;
    cfn<void,address,address,u32> retain;
    cfn<address,address,address,address> entry;
    cfn<address,address,u32> iterate;
    cfn<Row,address> next;
    cfn<void,address> drop_iterator;
    cfn<address> make_paths;
    cfn<void,address,address> insert_path;
    cfn<void,address,address> replace_paths;
    cfn<LocationView,address> location;
    cfn<PathStep,address,u64> step;
    cfn<address,address,Span> point_invocation;
    cfn<void,address,Span,address,address,address> put_point;
    cfn<Optional<SignatureNode>,address,Bytes> resolve;
    cfn<Optional<u32>,address,ExprRef> literal;
    cfn<void,address,address,address> finish;
};
extern "C" void run(SignatureNode function,Host host,address context) on cpu
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish);
extern "C" u32 retain_action(LocationView location,LocationView written,u32 name_only,
    cfn<PathStep,address,u64> step) on cpu requires nonzero(step);
