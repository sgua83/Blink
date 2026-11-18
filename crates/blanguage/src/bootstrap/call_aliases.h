module bootstrap.call_aliases;
struct ArgumentInfo {
 u32 exclusive;u32 value;u32 atomic;u32 declared_dataset;
 u32 stage_dataset;u32 reference;u32 owned_value;
};
struct StageInfo {u32 present;u64 binding;u64 count;};
struct StageWord {u32 start;u32 end;};
struct RegionInfo {u32 present;u64 sources;u64 words;};
struct HandleValues {address values;u64 count;};
struct AliasHost {
 cfn<ArgumentInfo,address,u64> argument;
 cfn<StageInfo,address,u64> stage;
 cfn<StageWord,address,u64,u64> stage_word;
 cfn<u32,address,u64,u64> place_overlap;
 cfn<RegionInfo,address,u64> region;
 cfn<u32,address,u64,u64,u64,u64> source_equal;
 cfn<u32,address,u64,u64> region_word;
 cfn<u32,address,u64,u64,cfn<u32,HandleValues,HandleValues,cfn<u32,address,u64,address,u64>>
  requires nonzero(argument2)>
  requires nonzero(argument3) with_handles;
};
extern "C" u64 first_conflict(address context,u64 count,AliasHost host) on cpu
 requires nonzero(host.argument)
 requires nonzero(host.stage)
 requires nonzero(host.stage_word)
 requires nonzero(host.place_overlap)
 requires nonzero(host.region)
 requires nonzero(host.source_equal)
 requires nonzero(host.region_word)
 requires nonzero(host.with_handles);
