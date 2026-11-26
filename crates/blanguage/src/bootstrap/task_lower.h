module bootstrap.task_lower;
import bootstrap.ir_dataflow;
enum Kind:u32 {Other=0;Start=1;Scope=2;Pair=3;};
enum Role:u32 {Ordinary=0;Frame=1;ReturnSource=2;Completion=3;};
struct BindHost {
 cfn<u32,address> root;
 cfn<u32,address,usize> function;
};
struct Instruction {Kind kind;address first;address second;};
struct RewriteHost {
 cfn<Instruction,address,usize> inspect;
 cfn<u32,address,address> child;
 cfn<u32,address,address,usize> start;
};
struct ScanHost {
 Host ir;
 cfn<void,address,address> address;
 cfn<u32,address,address> start;
};
extern "C" u32 scan_starts(Span code,address context,u32 rewrite,ScanHost host) on cpu
 requires nonzero(host.ir.inspect) requires nonzero(host.address) requires nonzero(host.start);
struct BoundStart { usize binding; usize worker_argument; usize retained_arguments; };
struct StartHost {
 cfn<u32,address,usize> matches;
 cfn<void,address> create;
};
enum BindingParameter:u32 { Frame=0; Value=1; Reference=2; InOut=3; };
struct PositionHost {
 cfn<BindingParameter,address,usize> parameter;
 cfn<u32,address,usize> owned;
 cfn<void,address,usize,usize,usize,BindingParameter> publish;
};
struct EntrySpec {
 usize parameters;
 usize fields;
 usize results;
 u32 result_bytes;
 u32 has_result;
 u32 fallible;
 u32 selector;
};
struct EntryParameter { Role role; u32 reference; u32 size; usize components; };
struct EntryField { address ty; u32 offset; usize buffer; };
struct EntryAddress { usize low; usize high; };
struct EntryCall { usize result; usize error; };
enum EntryStatus:u32 { Complete=0; MissingResult=1; MissingCompletion=2; MissingArgument=3; BadAddress=4; BadType=5; Unused=6; };
struct EntryHost {
 cfn<EntryParameter,address,usize> parameter;
 cfn<EntryField,address,usize> field;
 cfn<u32,address,usize,address,u32> valid;
 cfn<EntryAddress,address,EntryField> pointer;
 cfn<void,address,usize,Role,EntryAddress,u32> reference;
 cfn<usize,address,usize,usize,EntryField> load_value;
 cfn<void,address,usize,usize,usize> value;
 cfn<EntryCall,address,u32> call;
 cfn<void,address,usize> raise_error;
 cfn<void,address,usize,usize> store;
 cfn<void,address> selector;
};
extern "C" u32 bind(address context,u32 root,usize functions,BindHost host) on cpu
 requires nonzero(host.root) requires nonzero(host.function);
extern "C" u32 rewrite_block(address block,usize count,address context,RewriteHost host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.child) requires nonzero(host.start);
extern "C" BoundStart bind_start(address context,usize count,StartHost host) on cpu
 requires nonzero(host.matches) requires nonzero(host.create);
extern "C" void bind_positions(address context,usize fields,usize parameters,usize first,PositionHost host) on cpu
 requires nonzero(host.parameter) requires nonzero(host.owned) requires nonzero(host.publish);
extern "C" EntryStatus emit_entry(address context,EntrySpec spec,EntryHost host) on cpu
 requires nonzero(host.parameter) requires nonzero(host.field) requires nonzero(host.valid)
 requires nonzero(host.pointer) requires nonzero(host.reference) requires nonzero(host.load_value)
 requires nonzero(host.value) requires nonzero(host.call) requires nonzero(host.raise_error)
 requires nonzero(host.store) requires nonzero(host.selector);
