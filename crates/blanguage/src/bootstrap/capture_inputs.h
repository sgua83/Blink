module bootstrap.capture_inputs;
import bootstrap.names;

enum CaptureStatus:u32 { Failed=0; Ignored=1; Opened=2; };
struct CaptureOpened { NameBytes name; CaptureStatus status; };
struct CaptureImports { address data; usize count; };
struct CaptureHost {
    address context;
    cfn<CaptureOpened,address,NameBytes> open;
    cfn<CaptureImports,address,NameBytes,u32> bind;
    cfn<NameBytes,CaptureImports,usize> ensures memory(read) name;
    cfn<void,CaptureImports> close;
    cfn<u32,address,NameBytes> provider;
    cfn<u32,address,address,usize> export;
};
extern "C" i32 capture_build_inputs(CaptureHost host, usize requests) on cpu
    requires nonzero(host.open) requires nonzero(host.bind)
    requires nonzero(host.name) requires nonzero(host.close) requires nonzero(host.export);
extern "C" u32 capture_export_owner(CaptureHost host, address visited, NameBytes name) on cpu
    requires nonzero(host.open) requires nonzero(host.bind)
    requires nonzero(host.name) requires nonzero(host.close) requires nonzero(host.provider);
