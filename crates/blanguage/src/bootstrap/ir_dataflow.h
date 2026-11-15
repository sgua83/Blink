module bootstrap.ir_dataflow;
struct Node { address data; u32 kind; };
struct Span { address data; usize count; usize stride; };
struct View {
 u32 tag; usize a; usize b; usize c; usize d;
 Span first; Span second; Node left; Node right;
};
struct Host { cfn<View,Node> ensures memory(read) inspect; };
struct Word { u32 present; usize value; };
struct Results { Span first; Span second; usize a; usize b; usize inline_count; usize index; };
struct Locals {
 Node source; u32 mode; u32 writing; usize phase; usize entry; usize alias;
 usize position; usize end; u32 active;
};
extern "C" Results results(Node source,Host host) on cpu requires nonzero(host.inspect);
extern "C" Word next_result(inout Results state) on cpu;
extern "C" u32 operands(Node source,Host host,address context,cfn<u32,address,usize> visit) on cpu
 requires nonzero(host.inspect) requires nonzero(visit);
extern "C" Locals locals(Node source,u32 writing,Host host) on cpu requires nonzero(host.inspect);
extern "C" Word next_local(inout Locals state,Host host) on cpu requires nonzero(host.inspect);

// Exclusive, synchronous projections of the same original IR storage.
struct RewriteNode { address data; u32 kind; };
struct RewriteSpan { address data; usize count; usize stride; };
struct RewriteView {
 u32 tag; address a; address b; address c; address d;
 RewriteSpan first; RewriteSpan second; RewriteSpan third;
 RewriteNode left; RewriteNode right; usize length;
};
struct RewriteHost { cfn<RewriteView,RewriteNode> ensures memory(read) inspect; };
struct RewriteMap {
 address context;
 cfn<usize,address,usize> ensures memory(read) reg;
 cfn<usize,address,usize> ensures memory(read) slot;
};
struct RewriteInputMap { address context; cfn<usize,address,usize> ensures memory(read) reg; };
extern "C" void rewrite_code(RewriteSpan code,RewriteHost host,RewriteMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg) requires nonzero(mapping.slot);
extern "C" void rewrite_operands(RewriteNode source,RewriteHost host,RewriteInputMap mapping) on cpu
 requires nonzero(host.inspect) requires nonzero(mapping.reg);
