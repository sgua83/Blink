module bootstrap.contract_codec;
import codec.value_format;

enum IdentityKind:u32 { Register=0; DataLength=1; TileLength=2; ParameterContent=3; ResourceParameter=4; ParameterStorage=5; ElementComponent=6; ElementParent=7; ElementIndex=8; DatasetContent=9; MemoryValue=10; LoopValue=11; TaskResult=12; TaskCompletion=13; TaskWorkCompletion=14; TaskControl=15; TaskReturnSource=16; };

enum OperandKind:u32 { Value=0; Constant=1; };

enum IntegerKind:u32 { Add=0; Subtract=1; Multiply=2; Divide=3; Remainder=4; And=5; Or=6; Xor=7; ShiftLeft=8; ShiftRight=9; };

enum ComparisonKind:u32 { Equal=0; NotEqual=1; Less=2; Greater=3; LessEqual=4; GreaterEqual=5; };

enum FloatingKind:u32 { Cast=0; Upper=1; };

enum DomainKind:u32 { Data=0; Fixed=1; Tile=2; DataLength=3; TileLength=4; View=5; ViewLength=6; ApplicationData=7; ApplicationLength=8; Upper=9; Lower=10; Room=11; Sum=12; Offset=13; Shifted=14; Computed=15; CommonRange=16; KnownBits=17; Floating=18; };

enum FactKind:u32 { Equal=0; Index=1; Minimum=2; Maximum=3; NonZero=4; Boolean=5; Below=6; Outside=7; Zero=8; NotEqual=9; Predicate=10; };

struct NumericIdentity { IdentityKind kind; u64 first; u64 second; u64 third; };

struct NumericOperand { OperandKind kind; NumericIdentity identity; ScalarTag scalar; u64 bits; };

struct NumericSelection { u32 affine; NumericOperand operand; NumericIdentity base; u64 scale_low; u64 scale_high; u64 offset_low; u64 offset_high; };

struct NumericExpression { u32 form; IntegerKind operation; ScalarTag scalar; NumericOperand left; NumericOperand right; NumericSelection selection; };

struct NumericFloating { NumericIdentity source; u64 lower; u64 upper; array<u64,3> terms; ScalarTag scalar; FloatingKind form; };

struct NumericPredicate { ComparisonKind operation; ScalarTag scalar; NumericOperand left; NumericOperand right; };

struct NumericInterval { u64 lower; u64 upper; };

struct RangeWriter { cfn<void,address,address,u64> append; cfn<NumericInterval,address> ensures memory(read) interval; };

struct DomainFields {DomainKind kind; address first; address second; address third; u64 ordinal; u32 distance; array<ScalarTag,4> scalars; u64 mask; u64 bits; address expression; address floating;};

struct FactFields {FactKind kind; address first; address second; address domain; ScalarTag scalar; u64 bits; u32 truth; address predicate;};

struct NumericWriter {
 cfn<void,address,address,u64> append;
 cfn<NumericIdentity,address> ensures memory(read) identity;
 cfn<NumericExpression,address> ensures memory(read) expression;
 cfn<NumericFloating,address> ensures memory(read) floating;
 cfn<NumericPredicate,address> ensures memory(read) predicate;
 cfn<DomainFields,address> ensures memory(read) domain;
 cfn<FactFields,address> ensures memory(read) fact;
};

struct NumericReader {address context;cfn<ReadStatus,address,address,DomainKind> domain;cfn<ReadStatus,address,address,FactKind> fact;};

enum NodeKind:u32 { Impossible=0; Facts=1; Both=2; Either=3; };

enum ClauseKind:u32 { Impossible=0; Facts=1; Both=2; Either=3; Branch=4; };

struct ClauseFields {u32 present;ClauseKind kind;address source;address facts;u64 first;u64 second;};

struct NodeFields {NodeKind kind;address facts;u64 first;u64 second;};

struct RootFields {u32 present;u64 first;u64 second;};

struct Inserted {u64 index;u32 inserted;};

struct CanonicalHost {
 cfn<ClauseFields,address> next;
 cfn<u64,address,u64> ensures memory(read) index;
 cfn<u64,address> ensures memory(read) len;
 cfn<Inserted,address,NodeKind,address,u64,u64,u64> intern;
 cfn<void,address,address,NodeKind,address,u64,u64> record;
 cfn<void,address,address,u64> bind;
};

struct ConditionWriter {
 cfn<void,address,address,u64> append;
 cfn<RootFields,address,u64> ensures memory(read) condition;
 cfn<NodeFields,address,u64> ensures memory(read) node;
 cfn<u64,address,u64> ensures memory(read) index;
 cfn<void,address,address> facts;
};

struct ConditionReader {
 address context;
 cfn<void,address> impossible;
 cfn<ReadStatus,address,address> facts;
 cfn<void,address,u64,u64> both;
 cfn<void,address,u64,u64> either;
};

struct ContractValueFields {address proof;address literal;address ty;address regs;address uniform;address constant;};

struct ContractValueWriter {cfn<void,address,address> proof;cfn<void,address,address> literal;cfn<void,address,address> ty;cfn<void,address,address> regs;cfn<void,address,address> uniform;cfn<void,address,address> constant;};

struct ContractValueReader {cfn<ReadStatus,address,address> proof;cfn<ReadStatus,address,address> literal;cfn<ReadStatus,address,address> ty;cfn<ReadStatus,address,address> regs;cfn<ReadStatus,address,address> uniform;cfn<ReadStatus,address,address> constant;};

struct ContractAddressLayoutFields {address size;address align;address parts;};

struct ContractAddressLayoutWriter {cfn<void,address,address> size;cfn<void,address,address> align;cfn<void,address,address> parts;};

struct ContractAddressLayoutReader {cfn<ReadStatus,address,address> size;cfn<ReadStatus,address,address> align;cfn<ReadStatus,address,address> parts;};

struct ContractAddressRootLocalFields {address base;address len;};

struct ContractAddressRootLocalWriter {cfn<void,address,address> base;cfn<void,address,address> len;};

struct ContractAddressRootLocalReader {cfn<ReadStatus,address,address> base;cfn<ReadStatus,address,address> len;};

struct ContractAddressRootParameterFields {address index;};

struct ContractAddressRootParameterWriter {cfn<void,address,address> index;};

struct ContractAddressRootParameterReader {cfn<ReadStatus,address,address> index;};

struct ContractAddressRootResourceFields {address borrow;address index;};

struct ContractAddressRootResourceWriter {cfn<void,address,address> borrow;cfn<void,address,address> index;};

struct ContractAddressRootResourceReader {cfn<ReadStatus,address,address> borrow;cfn<ReadStatus,address,address> index;};

struct ContractAddressRootBufferFields {address binding;};

struct ContractAddressRootBufferWriter {cfn<void,address,address> binding;};

struct ContractAddressRootBufferReader {cfn<ReadStatus,address,address> binding;};

struct ContractAddressRootDerivedFields {address borrow;};

struct ContractAddressRootDerivedWriter {cfn<void,address,address> borrow;};

struct ContractAddressRootDerivedReader {cfn<ReadStatus,address,address> borrow;};

struct ContractAddressRootExternalFields {address identity;};

struct ContractAddressRootExternalWriter {cfn<void,address,address> identity;};

struct ContractAddressRootExternalReader {cfn<ReadStatus,address,address> identity;};

struct ContractAddressRootDatasetFields {address borrow;address index;};

struct ContractAddressRootDatasetWriter {cfn<void,address,address> borrow;cfn<void,address,address> index;};

struct ContractAddressRootDatasetReader {cfn<ReadStatus,address,address> borrow;cfn<ReadStatus,address,address> index;};

struct ContractInitializationSourcePositionFields {address origin;};

struct ContractInitializationSourcePositionWriter {cfn<void,address,address> origin;};

struct ContractInitializationSourcePositionReader {cfn<ReadStatus,address,address> origin;};

struct ContractInitializationSourceTaskFields {address borrow;};

struct ContractInitializationSourceTaskWriter {cfn<void,address,address> borrow;};

struct ContractInitializationSourceTaskReader {cfn<ReadStatus,address,address> borrow;};

struct ContractInitializationFields {address source;address when;};

struct ContractInitializationWriter {cfn<void,address,address> source;cfn<void,address,address> when;};

struct ContractInitializationReader {cfn<ReadStatus,address,address> source;cfn<ReadStatus,address,address> when;};

struct ContractDataPayloadFields {address root;address ty;address length;address ownership;address access;address count;address residency;address layout;address elements;address prefixes;};

struct ContractDataPayloadWriter {cfn<void,address,address> root;cfn<void,address,address> ty;cfn<void,address,address> length;cfn<void,address,address> ownership;cfn<void,address,address> access;cfn<void,address,address> count;cfn<void,address,address> residency;cfn<void,address,address> layout;cfn<void,address,address> elements;cfn<void,address,address> prefixes;};

struct ContractDataPayloadReader {cfn<ReadStatus,address,address> root;cfn<ReadStatus,address,address> ty;cfn<ReadStatus,address,address> length;cfn<ReadStatus,address,address> ownership;cfn<ReadStatus,address,address> access;cfn<ReadStatus,address,address> count;cfn<ReadStatus,address,address> residency;cfn<ReadStatus,address,address> layout;cfn<ReadStatus,address,address> elements;cfn<ReadStatus,address,address> prefixes;};

struct ContractElementFactsKnownFields {address value;};

struct ContractElementFactsKnownWriter {cfn<void,address,address> value;};

struct ContractElementFactsKnownReader {cfn<ReadStatus,address,address> value;};

struct ContractElementFactsCleanupFields {address when;};

struct ContractElementFactsCleanupWriter {cfn<void,address,address> when;};

struct ContractElementFactsCleanupReader {cfn<ReadStatus,address,address> when;};

struct ContractElementFactsCommonFields {address a;address b;};

struct ContractElementFactsCommonWriter {cfn<void,address,address> a;cfn<void,address,address> b;};

struct ContractElementFactsCommonReader {cfn<ReadStatus,address,address> a;cfn<ReadStatus,address,address> b;};

struct ContractElementFactsSelectedFields {address when;address a;address b;};

struct ContractElementFactsSelectedWriter {cfn<void,address,address> when;cfn<void,address,address> a;cfn<void,address,address> b;};

struct ContractElementFactsSelectedReader {cfn<ReadStatus,address,address> when;cfn<ReadStatus,address,address> a;cfn<ReadStatus,address,address> b;};

struct ContractElementFactsInputFields {address when;};

struct ContractElementFactsInputWriter {cfn<void,address,address> when;};

struct ContractElementFactsInputReader {cfn<ReadStatus,address,address> when;};

struct ContractDataLayoutParameterFields {address parameter;address stride;address offset;};

struct ContractDataLayoutParameterWriter {cfn<void,address,address> parameter;cfn<void,address,address> stride;cfn<void,address,address> offset;};

struct ContractDataLayoutParameterReader {cfn<ReadStatus,address,address> parameter;cfn<ReadStatus,address,address> stride;cfn<ReadStatus,address,address> offset;};

struct ContractReturnedValueFields {address value;};

struct ContractReturnedValueWriter {cfn<void,address,address> value;};

struct ContractReturnedValueReader {cfn<ReadStatus,address,address> value;};

struct ContractReturnedDatasetFields {address data;};

struct ContractReturnedDatasetWriter {cfn<void,address,address> data;};

struct ContractReturnedDatasetReader {cfn<ReadStatus,address,address> data;};

struct ContractReturnedOwnedValueFields {address value;};

struct ContractReturnedOwnedValueWriter {cfn<void,address,address> value;};

struct ContractReturnedOwnedValueReader {cfn<ReadStatus,address,address> value;};

struct ContractOwnedValuePayloadFields {address value;address payloads;address ownership;};

struct ContractOwnedValuePayloadWriter {cfn<void,address,address> value;cfn<void,address,address> payloads;cfn<void,address,address> ownership;};

struct ContractOwnedValuePayloadReader {cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> payloads;cfn<ReadStatus,address,address> ownership;};

struct ContractTaskGuardFields {address target;address ty;address clauses;address result;address model;address arguments;address returned;address inputs;};

struct ContractTaskGuardWriter {cfn<void,address,address> target;cfn<void,address,address> ty;cfn<void,address,address> clauses;cfn<void,address,address> result;cfn<void,address,address> model;cfn<void,address,address> arguments;cfn<void,address,address> returned;cfn<void,address,address> inputs;};

struct ContractTaskGuardReader {cfn<ReadStatus,address,address> target;cfn<ReadStatus,address,address> ty;cfn<ReadStatus,address,address> clauses;cfn<ReadStatus,address,address> result;cfn<ReadStatus,address,address> model;cfn<ReadStatus,address,address> arguments;cfn<ReadStatus,address,address> returned;cfn<ReadStatus,address,address> inputs;};

struct ContractTaskCompletionFields {address offset;address result;address layout;address task;address parent;};

struct ContractTaskCompletionWriter {cfn<void,address,address> offset;cfn<void,address,address> result;cfn<void,address,address> layout;cfn<void,address,address> task;cfn<void,address,address> parent;};

struct ContractTaskCompletionReader {cfn<ReadStatus,address,address> offset;cfn<ReadStatus,address,address> result;cfn<ReadStatus,address,address> layout;cfn<ReadStatus,address,address> task;cfn<ReadStatus,address,address> parent;};

struct ContractTaskProjectionReturnedFields {address component;address source;};

struct ContractTaskProjectionReturnedWriter {cfn<void,address,address> component;cfn<void,address,address> source;};

struct ContractTaskProjectionReturnedReader {cfn<ReadStatus,address,address> component;cfn<ReadStatus,address,address> source;};

struct ContractTaskProjectionInitializedFields {address returned;};

struct ContractTaskProjectionInitializedWriter {cfn<void,address,address> returned;};

struct ContractTaskProjectionInitializedReader {cfn<ReadStatus,address,address> returned;};

struct ContractExclusionPositionFields {address access;};

struct ContractExclusionPositionWriter {cfn<void,address,address> access;};

struct ContractExclusionPositionReader {cfn<ReadStatus,address,address> access;};

struct ContractPositionAccessFields {address layout;address written;address writable;address initialized;};

struct ContractPositionAccessWriter {cfn<void,address,address> layout;cfn<void,address,address> written;cfn<void,address,address> writable;cfn<void,address,address> initialized;};

struct ContractPositionAccessReader {cfn<ReadStatus,address,address> layout;cfn<ReadStatus,address,address> written;cfn<ReadStatus,address,address> writable;cfn<ReadStatus,address,address> initialized;};

struct ContractViewAccessResourceFields {address resources;address borrows;address kind;address when;};

struct ContractViewAccessResourceWriter {cfn<void,address,address> resources;cfn<void,address,address> borrows;cfn<void,address,address> kind;cfn<void,address,address> when;};

struct ContractViewAccessResourceReader {cfn<ReadStatus,address,address> resources;cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> kind;cfn<ReadStatus,address,address> when;};

struct ContractViewAccessPositionFields {address borrows;address when;address position;};

struct ContractViewAccessPositionWriter {cfn<void,address,address> borrows;cfn<void,address,address> when;cfn<void,address,address> position;};

struct ContractViewAccessPositionReader {cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> when;cfn<ReadStatus,address,address> position;};

struct ContractViewAccessMemoryFields {address borrows;address when;address access;};

struct ContractViewAccessMemoryWriter {cfn<void,address,address> borrows;cfn<void,address,address> when;cfn<void,address,address> access;};

struct ContractViewAccessMemoryReader {cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> when;cfn<ReadStatus,address,address> access;};

struct ContractViewAccessDatasetFields {address borrows;address when;address index;address access;address atomic;};

struct ContractViewAccessDatasetWriter {cfn<void,address,address> borrows;cfn<void,address,address> when;cfn<void,address,address> index;cfn<void,address,address> access;cfn<void,address,address> atomic;};

struct ContractViewAccessDatasetReader {cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> when;cfn<ReadStatus,address,address> index;cfn<ReadStatus,address,address> access;cfn<ReadStatus,address,address> atomic;};

struct ContractViewAccessInitializedFields {address borrows;address initialized;address when;};

struct ContractViewAccessInitializedWriter {cfn<void,address,address> borrows;cfn<void,address,address> initialized;cfn<void,address,address> when;};

struct ContractViewAccessInitializedReader {cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> initialized;cfn<ReadStatus,address,address> when;};

struct ContractViewAccessStoredFields {address borrows;address word;address value;address when;};

struct ContractViewAccessStoredWriter {cfn<void,address,address> borrows;cfn<void,address,address> word;cfn<void,address,address> value;cfn<void,address,address> when;};

struct ContractViewAccessStoredReader {cfn<ReadStatus,address,address> borrows;cfn<ReadStatus,address,address> word;cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> when;};

struct ContractResidencyPartFieldFields {address field;};

struct ContractResidencyPartFieldWriter {cfn<void,address,address> field;};

struct ContractResidencyPartFieldReader {cfn<ReadStatus,address,address> field;};

struct ContractResidencyPartSelectedFields {address index;};

struct ContractResidencyPartSelectedWriter {cfn<void,address,address> index;};

struct ContractResidencyPartSelectedReader {cfn<ReadStatus,address,address> index;};

struct ContractResidencyInputFields {address parameter;address fields;};

struct ContractResidencyInputWriter {cfn<void,address,address> parameter;cfn<void,address,address> fields;};

struct ContractResidencyInputReader {cfn<ReadStatus,address,address> parameter;cfn<ReadStatus,address,address> fields;};

struct ContractResidencyOriginInputFields {address input;};

struct ContractResidencyOriginInputWriter {cfn<void,address,address> input;};

struct ContractResidencyOriginInputReader {cfn<ReadStatus,address,address> input;};

struct ContractResidencyOriginOutputFields {address output;};

struct ContractResidencyOriginOutputWriter {cfn<void,address,address> output;};

struct ContractResidencyOriginOutputReader {cfn<ReadStatus,address,address> output;};

struct ContractResidencyOriginsFields {address origins;};

struct ContractResidencyOriginsWriter {cfn<void,address,address> origins;};

struct ContractResidencyOriginsReader {cfn<ReadStatus,address,address> origins;};

struct ContractResidencySelectedFields {address when;address a;address b;};

struct ContractResidencySelectedWriter {cfn<void,address,address> when;cfn<void,address,address> a;cfn<void,address,address> b;};

struct ContractResidencySelectedReader {cfn<ReadStatus,address,address> when;cfn<ReadStatus,address,address> a;cfn<ReadStatus,address,address> b;};

struct ContractResidencyOutputKindWrittenFields {address parameter;};

struct ContractResidencyOutputKindWrittenWriter {cfn<void,address,address> parameter;};

struct ContractResidencyOutputKindWrittenReader {cfn<ReadStatus,address,address> parameter;};

struct ContractResidencyOutputFields {address function;address kind;address fields;address arguments;address bindings;};

struct ContractResidencyOutputWriter {cfn<void,address,address> function;cfn<void,address,address> kind;cfn<void,address,address> fields;cfn<void,address,address> arguments;cfn<void,address,address> bindings;};

struct ContractResidencyOutputReader {cfn<ReadStatus,address,address> function;cfn<ReadStatus,address,address> kind;cfn<ReadStatus,address,address> fields;cfn<ReadStatus,address,address> arguments;cfn<ReadStatus,address,address> bindings;};

struct ContractDomainOutputFields {address binding;address domain;address words;};

struct ContractDomainOutputWriter {cfn<void,address,address> binding;cfn<void,address,address> domain;cfn<void,address,address> words;};

struct ContractDomainOutputReader {cfn<ReadStatus,address,address> binding;cfn<ReadStatus,address,address> domain;cfn<ReadStatus,address,address> words;};

struct ContractDomainInterfaceFields {address instance;address target;address buffers;address textures;address write_regions;address uniforms;address length_requirements;address element_requirements;address prefix_guarantees;address outputs;address elements;};

struct ContractDomainInterfaceWriter {cfn<void,address,address> instance;cfn<void,address,address> target;cfn<void,address,address> buffers;cfn<void,address,address> textures;cfn<void,address,address> write_regions;cfn<void,address,address> uniforms;cfn<void,address,address> length_requirements;cfn<void,address,address> element_requirements;cfn<void,address,address> prefix_guarantees;cfn<void,address,address> outputs;cfn<void,address,address> elements;};

struct ContractDomainInterfaceReader {cfn<ReadStatus,address,address> instance;cfn<ReadStatus,address,address> target;cfn<ReadStatus,address,address> buffers;cfn<ReadStatus,address,address> textures;cfn<ReadStatus,address,address> write_regions;cfn<ReadStatus,address,address> uniforms;cfn<ReadStatus,address,address> length_requirements;cfn<ReadStatus,address,address> element_requirements;cfn<ReadStatus,address,address> prefix_guarantees;cfn<ReadStatus,address,address> outputs;cfn<ReadStatus,address,address> elements;};

struct ContractRelationOpPositionFields {address origin;};

struct ContractRelationOpPositionWriter {cfn<void,address,address> origin;};

struct ContractRelationOpPositionReader {cfn<ReadStatus,address,address> origin;};

struct ContractRelationOpReadFields {address word;address frame;};

struct ContractRelationOpReadWriter {cfn<void,address,address> word;cfn<void,address,address> frame;};

struct ContractRelationOpReadReader {cfn<ReadStatus,address,address> word;cfn<ReadStatus,address,address> frame;};

struct ContractRelationOpElementFields {address fields;address index;};

struct ContractRelationOpElementWriter {cfn<void,address,address> fields;cfn<void,address,address> index;};

struct ContractRelationOpElementReader {cfn<ReadStatus,address,address> fields;cfn<ReadStatus,address,address> index;};

struct ContractRelationOpProjectionFields {address fields;};

struct ContractRelationOpProjectionWriter {cfn<void,address,address> fields;};

struct ContractRelationOpProjectionReader {cfn<ReadStatus,address,address> fields;};

struct ContractRelationOpSelectFields {address indices;};

struct ContractRelationOpSelectWriter {cfn<void,address,address> indices;};

struct ContractRelationOpSelectReader {cfn<ReadStatus,address,address> indices;};

struct ContractRelationOpInitialFields {address origin;address frame;};

struct ContractRelationOpInitialWriter {cfn<void,address,address> origin;cfn<void,address,address> frame;};

struct ContractRelationOpInitialReader {cfn<ReadStatus,address,address> origin;cfn<ReadStatus,address,address> frame;};

struct ContractRelationEdgeFields {address from;address op;address when;};

struct ContractRelationEdgeWriter {cfn<void,address,address> from;cfn<void,address,address> op;cfn<void,address,address> when;};

struct ContractRelationEdgeReader {cfn<ReadStatus,address,address> from;cfn<ReadStatus,address,address> op;cfn<ReadStatus,address,address> when;};

struct ContractRelationNodeFields {address seeds;address edges;};

struct ContractRelationNodeWriter {cfn<void,address,address> seeds;cfn<void,address,address> edges;};

struct ContractRelationNodeReader {cfn<ReadStatus,address,address> seeds;cfn<ReadStatus,address,address> edges;};

struct ContractRelationWriteFields {address target;address fields;address when;};

struct ContractRelationWriteWriter {cfn<void,address,address> target;cfn<void,address,address> fields;cfn<void,address,address> when;};

struct ContractRelationWriteReader {cfn<ReadStatus,address,address> target;cfn<ReadStatus,address,address> fields;cfn<ReadStatus,address,address> when;};

struct ContractRelationFrameFields {address parent;address writes;};

struct ContractRelationFrameWriter {cfn<void,address,address> parent;cfn<void,address,address> writes;};

struct ContractRelationFrameReader {cfn<ReadStatus,address,address> parent;cfn<ReadStatus,address,address> writes;};

struct ContractRelationGraphFields {address nodes;address variables;address frames;};

struct ContractRelationGraphWriter {cfn<void,address,address> nodes;cfn<void,address,address> variables;cfn<void,address,address> frames;};

struct ContractRelationGraphReader {cfn<ReadStatus,address,address> nodes;cfn<ReadStatus,address,address> variables;cfn<ReadStatus,address,address> frames;};

struct ContractAddressIdentityConstantFields {address bits;};

struct ContractAddressIdentityConstantWriter {cfn<void,address,address> bits;};

struct ContractAddressIdentityConstantReader {cfn<ReadStatus,address,address> bits;};

struct ContractAddressIdentityValueFields {address ids;};

struct ContractAddressIdentityValueWriter {cfn<void,address,address> ids;};

struct ContractAddressIdentityValueReader {cfn<ReadStatus,address,address> ids;};

struct ContractAddressIdentityStorageFields {address borrow;};

struct ContractAddressIdentityStorageWriter {cfn<void,address,address> borrow;};

struct ContractAddressIdentityStorageReader {cfn<ReadStatus,address,address> borrow;};

struct ContractAddressOriginFields {address root;address layout;address base;address words;address writable;address initialized;};

struct ContractAddressOriginWriter {cfn<void,address,address> root;cfn<void,address,address> layout;cfn<void,address,address> base;cfn<void,address,address> words;cfn<void,address,address> writable;cfn<void,address,address> initialized;};

struct ContractAddressOriginReader {cfn<ReadStatus,address,address> root;cfn<ReadStatus,address,address> layout;cfn<ReadStatus,address,address> base;cfn<ReadStatus,address,address> words;cfn<ReadStatus,address,address> writable;cfn<ReadStatus,address,address> initialized;};

struct ContractBorrowSourceDataFields {address id;};

struct ContractBorrowSourceDataWriter {cfn<void,address,address> id;};

struct ContractBorrowSourceDataReader {cfn<ReadStatus,address,address> id;};

struct ContractBorrowSourceParameterFields {address id;address indices;};

struct ContractBorrowSourceParameterWriter {cfn<void,address,address> id;cfn<void,address,address> indices;};

struct ContractBorrowSourceParameterReader {cfn<ReadStatus,address,address> id;cfn<ReadStatus,address,address> indices;};

struct ContractBorrowSourcePositionFields {address origin;};

struct ContractBorrowSourcePositionWriter {cfn<void,address,address> origin;};

struct ContractBorrowSourcePositionReader {cfn<ReadStatus,address,address> origin;};

struct ContractBorrowSourceElementFields {address parent;address fields;address index;};

struct ContractBorrowSourceElementWriter {cfn<void,address,address> parent;cfn<void,address,address> fields;cfn<void,address,address> index;};

struct ContractBorrowSourceElementReader {cfn<ReadStatus,address,address> parent;cfn<ReadStatus,address,address> fields;cfn<ReadStatus,address,address> index;};

struct ContractBorrowSourceProjectionFields {address parent;address fields;};

struct ContractBorrowSourceProjectionWriter {cfn<void,address,address> parent;cfn<void,address,address> fields;};

struct ContractBorrowSourceProjectionReader {cfn<ReadStatus,address,address> parent;cfn<ReadStatus,address,address> fields;};

struct ContractBorrowSourceContentsFields {address origin;address word;};

struct ContractBorrowSourceContentsWriter {cfn<void,address,address> origin;cfn<void,address,address> word;};

struct ContractBorrowSourceContentsReader {cfn<ReadStatus,address,address> origin;cfn<ReadStatus,address,address> word;};

struct ContractBorrowSourceRelationFields {address graph;};

struct ContractBorrowSourceRelationWriter {cfn<void,address,address> graph;};

struct ContractBorrowSourceRelationReader {cfn<ReadStatus,address,address> graph;};

struct ContractBorrowSourceTaskFields {address captures;address outputs;address initialized;address guards;address completions;};

struct ContractBorrowSourceTaskWriter {cfn<void,address,address> captures;cfn<void,address,address> outputs;cfn<void,address,address> initialized;cfn<void,address,address> guards;cfn<void,address,address> completions;};

struct ContractBorrowSourceTaskReader {cfn<ReadStatus,address,address> captures;cfn<ReadStatus,address,address> outputs;cfn<ReadStatus,address,address> initialized;cfn<ReadStatus,address,address> guards;cfn<ReadStatus,address,address> completions;};

struct ContractBorrowSourceTaskOutputFields {address task;address projection;};

struct ContractBorrowSourceTaskOutputWriter {cfn<void,address,address> task;cfn<void,address,address> projection;};

struct ContractBorrowSourceTaskOutputReader {cfn<ReadStatus,address,address> task;cfn<ReadStatus,address,address> projection;};

struct ContractResidencyContractFields {address required;address calls;};

struct ContractResidencyContractWriter {cfn<void,address,address> required;cfn<void,address,address> calls;};

struct ContractResidencyContractReader {cfn<ReadStatus,address,address> required;cfn<ReadStatus,address,address> calls;};

struct ContractResidencyCallFields {address function;address arguments;address bindings;address path;};

struct ContractResidencyCallWriter {cfn<void,address,address> function;cfn<void,address,address> arguments;cfn<void,address,address> bindings;cfn<void,address,address> path;};

struct ContractResidencyCallReader {cfn<ReadStatus,address,address> function;cfn<ReadStatus,address,address> arguments;cfn<ReadStatus,address,address> bindings;cfn<ReadStatus,address,address> path;};

struct ContractOrdinaryContractFields {address parameters;address result;address inputs;address element_inputs;address returned;address written;address requires;address target;address frame;address fallible;address domain;address residency;address view_access;address initialization;};

struct ContractOrdinaryContractWriter {cfn<void,address,address> parameters;cfn<void,address,address> result;cfn<void,address,address> inputs;cfn<void,address,address> element_inputs;cfn<void,address,address> returned;cfn<void,address,address> written;cfn<void,address,address> requires;cfn<void,address,address> target;cfn<void,address,address> frame;cfn<void,address,address> fallible;cfn<void,address,address> domain;cfn<void,address,address> residency;cfn<void,address,address> view_access;cfn<void,address,address> initialization;};

struct ContractOrdinaryContractReader {cfn<ReadStatus,address,address> parameters;cfn<ReadStatus,address,address> result;cfn<ReadStatus,address,address> inputs;cfn<ReadStatus,address,address> element_inputs;cfn<ReadStatus,address,address> returned;cfn<ReadStatus,address,address> written;cfn<ReadStatus,address,address> requires;cfn<ReadStatus,address,address> target;cfn<ReadStatus,address,address> frame;cfn<ReadStatus,address,address> fallible;cfn<ReadStatus,address,address> domain;cfn<ReadStatus,address,address> residency;cfn<ReadStatus,address,address> view_access;cfn<ReadStatus,address,address> initialization;};

struct ContractProofFields {address origins;address conditions;address ranges;address nonzero;address domains;address views;address addresses;};

struct ContractProofWriter {cfn<void,address,address> origins;cfn<void,address,address> conditions;cfn<void,address,address> ranges;cfn<void,address,address> nonzero;cfn<void,address,address> domains;cfn<void,address,address> views;cfn<void,address,address> addresses;};

struct ContractProofReader {cfn<ReadStatus,address,address> origins;cfn<ReadStatus,address,address> conditions;cfn<ReadStatus,address,address> ranges;cfn<ReadStatus,address,address> nonzero;cfn<ReadStatus,address,address> domains;cfn<ReadStatus,address,address> views;cfn<ReadStatus,address,address> addresses;};

struct ContractAddressRootVariants{cfn<ReadStatus,address,address> local;cfn<ReadStatus,address,address> parameter;cfn<ReadStatus,address,address> resource;cfn<ReadStatus,address,address> buffer;cfn<ReadStatus,address,address> derived;cfn<ReadStatus,address,address> external;cfn<ReadStatus,address,address> dataset;};

struct ContractInitializationSourceVariants{cfn<ReadStatus,address,address> position;cfn<ReadStatus,address,address> task;};

struct ContractElementFactsVariants{cfn<ReadStatus,address,address> unknown;cfn<ReadStatus,address,address> known;cfn<ReadStatus,address,address> cleanup;cfn<ReadStatus,address,address> common;cfn<ReadStatus,address,address> selected;cfn<ReadStatus,address,address> input;};

struct ContractDataLayoutVariants{cfn<ReadStatus,address,address> declared;cfn<ReadStatus,address,address> parameter;};

struct ContractReturnedVariants{cfn<ReadStatus,address,address> unit;cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> dataset;cfn<ReadStatus,address,address> owned_value;};

struct ContractTaskProjectionVariants{cfn<ReadStatus,address,address> returned;cfn<ReadStatus,address,address> writes;cfn<ReadStatus,address,address> places;cfn<ReadStatus,address,address> initialized;};

struct ContractExclusionVariants{cfn<ReadStatus,address,address> resource;cfn<ReadStatus,address,address> ownership;cfn<ReadStatus,address,address> position;cfn<ReadStatus,address,address> view;};

struct ContractViewAccessVariants{cfn<ReadStatus,address,address> resource;cfn<ReadStatus,address,address> position;cfn<ReadStatus,address,address> memory;cfn<ReadStatus,address,address> dataset;cfn<ReadStatus,address,address> initialized;cfn<ReadStatus,address,address> stored;};

struct ContractResidencyPartVariants{cfn<ReadStatus,address,address> field;cfn<ReadStatus,address,address> elements;cfn<ReadStatus,address,address> selected;};

struct ContractResidencyOriginVariants{cfn<ReadStatus,address,address> cpu;cfn<ReadStatus,address,address> gpu;cfn<ReadStatus,address,address> input;cfn<ReadStatus,address,address> output;cfn<ReadStatus,address,address> task_transfer;};

struct ContractResidencyVariants{cfn<ReadStatus,address,address> absent;cfn<ReadStatus,address,address> origins;cfn<ReadStatus,address,address> selected;};

struct ContractResidencyOutputKindVariants{cfn<ReadStatus,address,address> returned;cfn<ReadStatus,address,address> written;};

struct ContractRelationOpVariants{cfn<ReadStatus,address,address> identity;cfn<ReadStatus,address,address> position;cfn<ReadStatus,address,address> read;cfn<ReadStatus,address,address> element;cfn<ReadStatus,address,address> projection;cfn<ReadStatus,address,address> resources;cfn<ReadStatus,address,address> owners;cfn<ReadStatus,address,address> select;cfn<ReadStatus,address,address> initial;};

struct ContractOwnershipVariants{cfn<ReadStatus,address,address> owned;cfn<ReadStatus,address,address> borrowed;cfn<ReadStatus,address,address> moved;};

struct ContractAddressIdentityVariants{cfn<ReadStatus,address,address> constant;cfn<ReadStatus,address,address> value;cfn<ReadStatus,address,address> storage;};

struct ContractBorrowSourceVariants{cfn<ReadStatus,address,address> data;cfn<ReadStatus,address,address> parameter;cfn<ReadStatus,address,address> unresolved;cfn<ReadStatus,address,address> inactive;cfn<ReadStatus,address,address> external;cfn<ReadStatus,address,address> position;cfn<ReadStatus,address,address> element;cfn<ReadStatus,address,address> projection;cfn<ReadStatus,address,address> contents;cfn<ReadStatus,address,address> relation;cfn<ReadStatus,address,address> task;cfn<ReadStatus,address,address> task_output;};

extern "C" void numeric_write_identity(NumericIdentity value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus numeric_read_identity(Bytes input,inout u64 position,inout NumericIdentity output) on cpu;

extern "C" void numeric_write_operand(NumericOperand value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus numeric_read_operand(Bytes input,inout u64 position,inout NumericOperand output) on cpu;

extern "C" void numeric_write_selection(NumericSelection value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);
extern "C" ReadStatus numeric_read_selection(Bytes input,inout u64 position,inout NumericSelection output) on cpu;

extern "C" void numeric_write_integer(IntegerKind value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus numeric_read_integer(Bytes input,inout u64 position,inout IntegerKind output) on cpu;

extern "C" void numeric_write_expression(NumericExpression value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus numeric_read_expression(Bytes input,inout u64 position,inout NumericExpression output) on cpu;

extern "C" void numeric_write_comparison(ComparisonKind value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus numeric_read_comparison(Bytes input,inout u64 position,inout ComparisonKind output) on cpu;

extern "C" void numeric_write_floating_form(FloatingKind value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus numeric_read_floating_form(Bytes input,inout u64 position,inout FloatingKind output) on cpu;

extern "C" void numeric_write_floating(NumericFloating value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus numeric_read_floating(Bytes input,inout u64 position,inout NumericFloating output) on cpu;

extern "C" void numeric_write_predicate(NumericPredicate value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" ReadStatus numeric_read_predicate(Bytes input,inout u64 position,inout NumericPredicate output) on cpu;

extern "C" void numeric_write_ranges(Span values,address output,RangeWriter host) on cpu requires nonzero(host.append) requires nonzero(host.interval);

extern "C" ReadStatus numeric_read_ranges(Bytes input,inout u64 position,address output,cfn<void,address,u64,u64> push) on cpu requires nonzero(push);

extern "C" void numeric_write_domain(address source,address output,NumericWriter host) on cpu requires nonzero(host.append) requires nonzero(host.identity) requires nonzero(host.expression) requires nonzero(host.floating) requires nonzero(host.domain);

extern "C" ReadStatus numeric_read_domain(Bytes input,inout u64 position,address output,NumericReader host) on cpu requires nonzero(host.domain);

extern "C" void numeric_write_fact(address source,address output,NumericWriter host) on cpu requires nonzero(host.append) requires nonzero(host.identity) requires nonzero(host.expression) requires nonzero(host.floating) requires nonzero(host.domain) requires nonzero(host.fact) requires nonzero(host.predicate);

extern "C" ReadStatus numeric_read_fact(Bytes input,inout u64 position,address output,NumericReader host) on cpu requires nonzero(host.fact);

extern "C" ReadStatus numeric_read_payload_ordinal(Bytes input,inout u64 position,inout u64 field0) on cpu;

extern "C" ReadStatus numeric_read_payload_count(Bytes input,inout u64 position,inout u32 field0) on cpu;

extern "C" ReadStatus numeric_read_payload_view_identities(Bytes input,inout u64 position,inout array<NumericIdentity,3> field0) on cpu;

extern "C" ReadStatus numeric_read_payload_identity_distance(Bytes input,inout u64 position,inout NumericIdentity field0,inout u32 field1) on cpu;

extern "C" ReadStatus numeric_read_payload_identity_pair(Bytes input,inout u64 position,inout NumericIdentity field0,inout NumericIdentity field1) on cpu;

extern "C" ReadStatus numeric_read_payload_shifted_identities(Bytes input,inout u64 position,inout NumericIdentity field0,inout NumericIdentity field1,inout u32 field2) on cpu;

extern "C" ReadStatus numeric_read_payload_common_range(Bytes input,inout u64 position,inout NumericIdentity field0,inout ScalarTag field1,inout ScalarTag field2,inout ScalarTag field3,inout ScalarTag field4) on cpu;

extern "C" ReadStatus numeric_read_payload_known_bits(Bytes input,inout u64 position,inout ScalarTag field0,inout u64 field1,inout u64 field2) on cpu;

extern "C" ReadStatus numeric_read_payload_identity_domain(Bytes input,inout u64 position,inout NumericIdentity field0,address field1,NumericReader host) on cpu requires nonzero(host.domain);

extern "C" ReadStatus numeric_read_payload_identity_scalar_bits(Bytes input,inout u64 position,inout NumericIdentity field0,inout ScalarTag field1,inout u64 field2) on cpu;

extern "C" ReadStatus numeric_read_payload_identity_truth(Bytes input,inout u64 position,inout NumericIdentity field0,inout u32 field1) on cpu;

extern "C" ReadStatus numeric_read_payload_ordered_identities(Bytes input,inout u64 position,inout NumericIdentity field0,inout NumericIdentity field1,inout u32 field2,inout ScalarTag field3) on cpu;

extern "C" ReadStatus numeric_read_payload_predicate_truth(Bytes input,inout u64 position,inout NumericPredicate field0,inout u32 field1) on cpu;

extern "C" void condition_node_ids(address context,CanonicalHost host,u32 keep_pairs) on cpu requires nonzero(host.next) requires nonzero(host.index) requires nonzero(host.len) requires nonzero(host.intern) requires nonzero(host.record) requires nonzero(host.bind);

extern "C" void write_proof_conditions(address context,u64 length,u64 count,address output,ConditionWriter host) on cpu requires nonzero(host.append) requires nonzero(host.condition) requires nonzero(host.node) requires nonzero(host.index) requires nonzero(host.facts);

extern "C" i32 compare_proof_condition(address left,address right,u64 left_count,u64 right_count,
 cfn<NodeFields,address,u64> node,cfn<RootFields,address,u64> root,cfn<u64,address,u64> index,
 cfn<i32,address,address> compare_facts) on cpu
 requires nonzero(node) requires nonzero(root) requires nonzero(index) requires nonzero(compare_facts);

extern "C" void write_fact_sequence(u64 count,address iterator,address output,NumericWriter host,cfn<address,address> next) on cpu requires nonzero(host.append) requires nonzero(host.identity) requires nonzero(host.expression) requires nonzero(host.floating) requires nonzero(host.domain) requires nonzero(host.fact) requires nonzero(host.predicate) requires nonzero(next);

extern "C" ReadStatus read_condition_nodes(Bytes input,inout u64 position,inout u32 present,address output,ConditionReader host) on cpu requires nonzero(host.impossible) requires nonzero(host.facts) requires nonzero(host.both) requires nonzero(host.either);

extern "C" ReadStatus read_fact_sequence(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address> push) on cpu requires nonzero(push);

extern "C" ReadStatus read_condition_roots(Bytes input,inout u64 position,address output,cfn<void,address,u32,u64,u64> push) on cpu requires nonzero(push);

extern "C" void contract_write_requirement(address key,address condition,address output,cfn<void,address,address> key_write,cfn<void,address,address> condition_write,cfn<void,address,address,u64> append) on cpu requires nonzero(key_write) requires nonzero(condition_write) requires nonzero(append);

extern "C" void contract_write_value(address output,ContractValueFields fields,ContractValueWriter host) on cpu requires nonzero(host.proof) requires nonzero(host.literal) requires nonzero(host.ty) requires nonzero(host.regs) requires nonzero(host.uniform) requires nonzero(host.constant);

extern "C" ReadStatus contract_read_value(address context,ContractValueFields fields,ContractValueReader host) on cpu requires nonzero(host.proof) requires nonzero(host.literal) requires nonzero(host.ty) requires nonzero(host.regs) requires nonzero(host.uniform) requires nonzero(host.constant);

extern "C" void contract_write_address_layout(address output,ContractAddressLayoutFields fields,ContractAddressLayoutWriter host) on cpu requires nonzero(host.size) requires nonzero(host.align) requires nonzero(host.parts);

extern "C" ReadStatus contract_read_address_layout(address context,ContractAddressLayoutFields fields,ContractAddressLayoutReader host) on cpu requires nonzero(host.size) requires nonzero(host.align) requires nonzero(host.parts);

extern "C" void contract_write_address_root_local(address output,ContractAddressRootLocalFields fields,ContractAddressRootLocalWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.base) requires nonzero(host.len) requires nonzero(append);

extern "C" ReadStatus contract_read_address_root_local(address context,ContractAddressRootLocalFields fields,ContractAddressRootLocalReader host) on cpu requires nonzero(host.base) requires nonzero(host.len);

extern "C" void contract_write_address_root_parameter(address output,ContractAddressRootParameterFields fields,ContractAddressRootParameterWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.index) requires nonzero(append);

extern "C" ReadStatus contract_read_address_root_parameter(address context,ContractAddressRootParameterFields fields,ContractAddressRootParameterReader host) on cpu requires nonzero(host.index);

extern "C" void contract_write_address_root_resource(address output,ContractAddressRootResourceFields fields,ContractAddressRootResourceWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(host.index) requires nonzero(append);

extern "C" ReadStatus contract_read_address_root_resource(address context,ContractAddressRootResourceFields fields,ContractAddressRootResourceReader host) on cpu requires nonzero(host.borrow) requires nonzero(host.index);

extern "C" void contract_write_address_root_buffer(address output,ContractAddressRootBufferFields fields,ContractAddressRootBufferWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.binding) requires nonzero(append);

extern "C" ReadStatus contract_read_address_root_buffer(address context,ContractAddressRootBufferFields fields,ContractAddressRootBufferReader host) on cpu requires nonzero(host.binding);

extern "C" void contract_write_address_root_derived(address output,ContractAddressRootDerivedFields fields,ContractAddressRootDerivedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(append);

extern "C" ReadStatus contract_read_address_root_derived(address context,ContractAddressRootDerivedFields fields,ContractAddressRootDerivedReader host) on cpu requires nonzero(host.borrow);

extern "C" void contract_write_address_root_external(address output,ContractAddressRootExternalFields fields,ContractAddressRootExternalWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.identity) requires nonzero(append);

extern "C" ReadStatus contract_read_address_root_external(address context,ContractAddressRootExternalFields fields,ContractAddressRootExternalReader host) on cpu requires nonzero(host.identity);

extern "C" void contract_write_address_root_dataset(address output,ContractAddressRootDatasetFields fields,ContractAddressRootDatasetWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(host.index) requires nonzero(append);

extern "C" ReadStatus contract_read_address_root_dataset(address context,ContractAddressRootDatasetFields fields,ContractAddressRootDatasetReader host) on cpu requires nonzero(host.borrow) requires nonzero(host.index);

extern "C" void contract_write_initialization_source_position(address output,ContractInitializationSourcePositionFields fields,ContractInitializationSourcePositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(append);

extern "C" ReadStatus contract_read_initialization_source_position(address context,ContractInitializationSourcePositionFields fields,ContractInitializationSourcePositionReader host) on cpu requires nonzero(host.origin);

extern "C" void contract_write_initialization_source_task(address output,ContractInitializationSourceTaskFields fields,ContractInitializationSourceTaskWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(append);

extern "C" ReadStatus contract_read_initialization_source_task(address context,ContractInitializationSourceTaskFields fields,ContractInitializationSourceTaskReader host) on cpu requires nonzero(host.borrow);

extern "C" void contract_write_initialization(address output,ContractInitializationFields fields,ContractInitializationWriter host) on cpu requires nonzero(host.source) requires nonzero(host.when);

extern "C" ReadStatus contract_read_initialization(address context,ContractInitializationFields fields,ContractInitializationReader host) on cpu requires nonzero(host.source) requires nonzero(host.when);

extern "C" void contract_write_data_payload(address output,ContractDataPayloadFields fields,ContractDataPayloadWriter host) on cpu requires nonzero(host.root) requires nonzero(host.ty) requires nonzero(host.length) requires nonzero(host.ownership) requires nonzero(host.access) requires nonzero(host.count) requires nonzero(host.residency) requires nonzero(host.layout) requires nonzero(host.elements) requires nonzero(host.prefixes);

extern "C" ReadStatus contract_read_data_payload(address context,ContractDataPayloadFields fields,ContractDataPayloadReader host) on cpu requires nonzero(host.root) requires nonzero(host.ty) requires nonzero(host.length) requires nonzero(host.ownership) requires nonzero(host.access) requires nonzero(host.count) requires nonzero(host.residency) requires nonzero(host.layout) requires nonzero(host.elements) requires nonzero(host.prefixes);

extern "C" void contract_write_element_facts_unknown(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_element_facts_known(address output,ContractElementFactsKnownFields fields,ContractElementFactsKnownWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.value) requires nonzero(append);

extern "C" ReadStatus contract_read_element_facts_known(address context,ContractElementFactsKnownFields fields,ContractElementFactsKnownReader host) on cpu requires nonzero(host.value);

extern "C" void contract_write_element_facts_cleanup(address output,ContractElementFactsCleanupFields fields,ContractElementFactsCleanupWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(append);

extern "C" ReadStatus contract_read_element_facts_cleanup(address context,ContractElementFactsCleanupFields fields,ContractElementFactsCleanupReader host) on cpu requires nonzero(host.when);

extern "C" void contract_write_element_facts_common(address output,ContractElementFactsCommonFields fields,ContractElementFactsCommonWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.a) requires nonzero(host.b) requires nonzero(append);

extern "C" ReadStatus contract_read_element_facts_common(address context,ContractElementFactsCommonFields fields,ContractElementFactsCommonReader host) on cpu requires nonzero(host.a) requires nonzero(host.b);

extern "C" void contract_write_element_facts_selected(address output,ContractElementFactsSelectedFields fields,ContractElementFactsSelectedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b) requires nonzero(append);

extern "C" ReadStatus contract_read_element_facts_selected(address context,ContractElementFactsSelectedFields fields,ContractElementFactsSelectedReader host) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b);

extern "C" void contract_write_element_facts_input(address output,ContractElementFactsInputFields fields,ContractElementFactsInputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(append);

extern "C" ReadStatus contract_read_element_facts_input(address context,ContractElementFactsInputFields fields,ContractElementFactsInputReader host) on cpu requires nonzero(host.when);

extern "C" void contract_write_data_layout_declared(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_data_layout_parameter(address output,ContractDataLayoutParameterFields fields,ContractDataLayoutParameterWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parameter) requires nonzero(host.stride) requires nonzero(host.offset) requires nonzero(append);

extern "C" ReadStatus contract_read_data_layout_parameter(address context,ContractDataLayoutParameterFields fields,ContractDataLayoutParameterReader host) on cpu requires nonzero(host.parameter) requires nonzero(host.stride) requires nonzero(host.offset);

extern "C" void contract_write_returned_unit(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_returned_value(address output,ContractReturnedValueFields fields,ContractReturnedValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.value) requires nonzero(append);

extern "C" ReadStatus contract_read_returned_value(address context,ContractReturnedValueFields fields,ContractReturnedValueReader host) on cpu requires nonzero(host.value);

extern "C" void contract_write_returned_dataset(address output,ContractReturnedDatasetFields fields,ContractReturnedDatasetWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.data) requires nonzero(append);

extern "C" ReadStatus contract_read_returned_dataset(address context,ContractReturnedDatasetFields fields,ContractReturnedDatasetReader host) on cpu requires nonzero(host.data);

extern "C" void contract_write_returned_owned_value(address output,ContractReturnedOwnedValueFields fields,ContractReturnedOwnedValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.value) requires nonzero(append);

extern "C" ReadStatus contract_read_returned_owned_value(address context,ContractReturnedOwnedValueFields fields,ContractReturnedOwnedValueReader host) on cpu requires nonzero(host.value);

extern "C" void contract_write_owned_value_payload(address output,ContractOwnedValuePayloadFields fields,ContractOwnedValuePayloadWriter host) on cpu requires nonzero(host.value) requires nonzero(host.payloads) requires nonzero(host.ownership);

extern "C" ReadStatus contract_read_owned_value_payload(address context,ContractOwnedValuePayloadFields fields,ContractOwnedValuePayloadReader host) on cpu requires nonzero(host.value) requires nonzero(host.payloads) requires nonzero(host.ownership);

extern "C" void contract_write_task_guard(address output,ContractTaskGuardFields fields,ContractTaskGuardWriter host) on cpu requires nonzero(host.target) requires nonzero(host.ty) requires nonzero(host.clauses) requires nonzero(host.result) requires nonzero(host.model) requires nonzero(host.arguments) requires nonzero(host.returned) requires nonzero(host.inputs);

extern "C" ReadStatus contract_read_task_guard(address context,ContractTaskGuardFields fields,ContractTaskGuardReader host) on cpu requires nonzero(host.target) requires nonzero(host.ty) requires nonzero(host.clauses) requires nonzero(host.result) requires nonzero(host.model) requires nonzero(host.arguments) requires nonzero(host.returned) requires nonzero(host.inputs);

extern "C" void contract_write_task_completion(address output,ContractTaskCompletionFields fields,ContractTaskCompletionWriter host) on cpu requires nonzero(host.offset) requires nonzero(host.result) requires nonzero(host.layout) requires nonzero(host.task) requires nonzero(host.parent);

extern "C" ReadStatus contract_read_task_completion(address context,ContractTaskCompletionFields fields,ContractTaskCompletionReader host) on cpu requires nonzero(host.offset) requires nonzero(host.result) requires nonzero(host.layout) requires nonzero(host.task) requires nonzero(host.parent);

extern "C" void contract_write_task_projection_returned(address output,ContractTaskProjectionReturnedFields fields,ContractTaskProjectionReturnedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.component) requires nonzero(host.source) requires nonzero(append);

extern "C" ReadStatus contract_read_task_projection_returned(address context,ContractTaskProjectionReturnedFields fields,ContractTaskProjectionReturnedReader host) on cpu requires nonzero(host.component) requires nonzero(host.source);

extern "C" void contract_write_task_projection_writes(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_task_projection_places(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_task_projection_initialized(address output,ContractTaskProjectionInitializedFields fields,ContractTaskProjectionInitializedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.returned) requires nonzero(append);

extern "C" ReadStatus contract_read_task_projection_initialized(address context,ContractTaskProjectionInitializedFields fields,ContractTaskProjectionInitializedReader host) on cpu requires nonzero(host.returned);

extern "C" void contract_write_exclusion_resource(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_exclusion_ownership(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_exclusion_position(address output,ContractExclusionPositionFields fields,ContractExclusionPositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.access) requires nonzero(append);

extern "C" ReadStatus contract_read_exclusion_position(address context,ContractExclusionPositionFields fields,ContractExclusionPositionReader host) on cpu requires nonzero(host.access);

extern "C" void contract_write_exclusion_view(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_position_access(address output,ContractPositionAccessFields fields,ContractPositionAccessWriter host) on cpu requires nonzero(host.layout) requires nonzero(host.written) requires nonzero(host.writable) requires nonzero(host.initialized);

extern "C" ReadStatus contract_read_position_access(address context,ContractPositionAccessFields fields,ContractPositionAccessReader host) on cpu requires nonzero(host.layout) requires nonzero(host.written) requires nonzero(host.writable) requires nonzero(host.initialized);

extern "C" void contract_write_view_access_resource(address output,ContractViewAccessResourceFields fields,ContractViewAccessResourceWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.resources) requires nonzero(host.borrows) requires nonzero(host.kind) requires nonzero(host.when) requires nonzero(append);

extern "C" ReadStatus contract_read_view_access_resource(address context,ContractViewAccessResourceFields fields,ContractViewAccessResourceReader host) on cpu requires nonzero(host.resources) requires nonzero(host.borrows) requires nonzero(host.kind) requires nonzero(host.when);

extern "C" void contract_write_view_access_position(address output,ContractViewAccessPositionFields fields,ContractViewAccessPositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.position) requires nonzero(append);

extern "C" ReadStatus contract_read_view_access_position(address context,ContractViewAccessPositionFields fields,ContractViewAccessPositionReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.position);

extern "C" void contract_write_view_access_memory(address output,ContractViewAccessMemoryFields fields,ContractViewAccessMemoryWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.access) requires nonzero(append);

extern "C" ReadStatus contract_read_view_access_memory(address context,ContractViewAccessMemoryFields fields,ContractViewAccessMemoryReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.access);

extern "C" void contract_write_view_access_dataset(address output,ContractViewAccessDatasetFields fields,ContractViewAccessDatasetWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.index) requires nonzero(host.access) requires nonzero(host.atomic) requires nonzero(append);

extern "C" ReadStatus contract_read_view_access_dataset(address context,ContractViewAccessDatasetFields fields,ContractViewAccessDatasetReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.index) requires nonzero(host.access) requires nonzero(host.atomic);

extern "C" void contract_write_view_access_initialized(address output,ContractViewAccessInitializedFields fields,ContractViewAccessInitializedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.initialized) requires nonzero(host.when) requires nonzero(append);

extern "C" ReadStatus contract_read_view_access_initialized(address context,ContractViewAccessInitializedFields fields,ContractViewAccessInitializedReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.initialized) requires nonzero(host.when);

extern "C" void contract_write_view_access_stored(address output,ContractViewAccessStoredFields fields,ContractViewAccessStoredWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.word) requires nonzero(host.value) requires nonzero(host.when) requires nonzero(append);

extern "C" ReadStatus contract_read_view_access_stored(address context,ContractViewAccessStoredFields fields,ContractViewAccessStoredReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.word) requires nonzero(host.value) requires nonzero(host.when);

extern "C" void contract_write_residency_part_field(address output,ContractResidencyPartFieldFields fields,ContractResidencyPartFieldWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.field) requires nonzero(append);

extern "C" ReadStatus contract_read_residency_part_field(address context,ContractResidencyPartFieldFields fields,ContractResidencyPartFieldReader host) on cpu requires nonzero(host.field);

extern "C" void contract_write_residency_part_elements(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_residency_part_selected(address output,ContractResidencyPartSelectedFields fields,ContractResidencyPartSelectedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.index) requires nonzero(append);

extern "C" ReadStatus contract_read_residency_part_selected(address context,ContractResidencyPartSelectedFields fields,ContractResidencyPartSelectedReader host) on cpu requires nonzero(host.index);

extern "C" void contract_write_residency_input(address output,ContractResidencyInputFields fields,ContractResidencyInputWriter host) on cpu requires nonzero(host.parameter) requires nonzero(host.fields);

extern "C" ReadStatus contract_read_residency_input(address context,ContractResidencyInputFields fields,ContractResidencyInputReader host) on cpu requires nonzero(host.parameter) requires nonzero(host.fields);

extern "C" void contract_write_residency_origin_cpu(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_residency_origin_gpu(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_residency_origin_input(address output,ContractResidencyOriginInputFields fields,ContractResidencyOriginInputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.input) requires nonzero(append);

extern "C" ReadStatus contract_read_residency_origin_input(address context,ContractResidencyOriginInputFields fields,ContractResidencyOriginInputReader host) on cpu requires nonzero(host.input);

extern "C" void contract_write_residency_origin_output(address output,ContractResidencyOriginOutputFields fields,ContractResidencyOriginOutputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.output) requires nonzero(append);

extern "C" ReadStatus contract_read_residency_origin_output(address context,ContractResidencyOriginOutputFields fields,ContractResidencyOriginOutputReader host) on cpu requires nonzero(host.output);

extern "C" void contract_write_residency_origin_task_transfer(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_residency_absent(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_residency_origins(address output,ContractResidencyOriginsFields fields,ContractResidencyOriginsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origins) requires nonzero(append);

extern "C" ReadStatus contract_read_residency_origins(address context,ContractResidencyOriginsFields fields,ContractResidencyOriginsReader host) on cpu requires nonzero(host.origins);

extern "C" void contract_write_residency_selected(address output,ContractResidencySelectedFields fields,ContractResidencySelectedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b) requires nonzero(append);

extern "C" ReadStatus contract_read_residency_selected(address context,ContractResidencySelectedFields fields,ContractResidencySelectedReader host) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b);

extern "C" void contract_write_residency_output_kind_returned(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_residency_output_kind_written(address output,ContractResidencyOutputKindWrittenFields fields,ContractResidencyOutputKindWrittenWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parameter) requires nonzero(append);

extern "C" ReadStatus contract_read_residency_output_kind_written(address context,ContractResidencyOutputKindWrittenFields fields,ContractResidencyOutputKindWrittenReader host) on cpu requires nonzero(host.parameter);

extern "C" void contract_write_residency_output(address output,ContractResidencyOutputFields fields,ContractResidencyOutputWriter host) on cpu requires nonzero(host.function) requires nonzero(host.kind) requires nonzero(host.fields) requires nonzero(host.arguments) requires nonzero(host.bindings);

extern "C" ReadStatus contract_read_residency_output(address context,ContractResidencyOutputFields fields,ContractResidencyOutputReader host) on cpu requires nonzero(host.function) requires nonzero(host.kind) requires nonzero(host.fields) requires nonzero(host.arguments) requires nonzero(host.bindings);

extern "C" void contract_write_domain_output(address output,ContractDomainOutputFields fields,ContractDomainOutputWriter host) on cpu requires nonzero(host.binding) requires nonzero(host.domain) requires nonzero(host.words);

extern "C" ReadStatus contract_read_domain_output(address context,ContractDomainOutputFields fields,ContractDomainOutputReader host) on cpu requires nonzero(host.binding) requires nonzero(host.domain) requires nonzero(host.words);

extern "C" void contract_write_domain_interface(address output,ContractDomainInterfaceFields fields,ContractDomainInterfaceWriter host) on cpu requires nonzero(host.instance) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.textures) requires nonzero(host.write_regions) requires nonzero(host.uniforms) requires nonzero(host.length_requirements) requires nonzero(host.element_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.outputs) requires nonzero(host.elements);

extern "C" ReadStatus contract_read_domain_interface(address context,ContractDomainInterfaceFields fields,ContractDomainInterfaceReader host) on cpu requires nonzero(host.instance) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.textures) requires nonzero(host.write_regions) requires nonzero(host.uniforms) requires nonzero(host.length_requirements) requires nonzero(host.element_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.outputs) requires nonzero(host.elements);

extern "C" void contract_write_relation_op_identity(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_relation_op_position(address output,ContractRelationOpPositionFields fields,ContractRelationOpPositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(append);

extern "C" ReadStatus contract_read_relation_op_position(address context,ContractRelationOpPositionFields fields,ContractRelationOpPositionReader host) on cpu requires nonzero(host.origin);

extern "C" void contract_write_relation_op_read(address output,ContractRelationOpReadFields fields,ContractRelationOpReadWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.word) requires nonzero(host.frame) requires nonzero(append);

extern "C" ReadStatus contract_read_relation_op_read(address context,ContractRelationOpReadFields fields,ContractRelationOpReadReader host) on cpu requires nonzero(host.word) requires nonzero(host.frame);

extern "C" void contract_write_relation_op_element(address output,ContractRelationOpElementFields fields,ContractRelationOpElementWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.fields) requires nonzero(host.index) requires nonzero(append);

extern "C" ReadStatus contract_read_relation_op_element(address context,ContractRelationOpElementFields fields,ContractRelationOpElementReader host) on cpu requires nonzero(host.fields) requires nonzero(host.index);

extern "C" void contract_write_relation_op_projection(address output,ContractRelationOpProjectionFields fields,ContractRelationOpProjectionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.fields) requires nonzero(append);

extern "C" ReadStatus contract_read_relation_op_projection(address context,ContractRelationOpProjectionFields fields,ContractRelationOpProjectionReader host) on cpu requires nonzero(host.fields);

extern "C" void contract_write_relation_op_resources(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_relation_op_owners(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_relation_op_select(address output,ContractRelationOpSelectFields fields,ContractRelationOpSelectWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.indices) requires nonzero(append);

extern "C" ReadStatus contract_read_relation_op_select(address context,ContractRelationOpSelectFields fields,ContractRelationOpSelectReader host) on cpu requires nonzero(host.indices);

extern "C" void contract_write_relation_op_initial(address output,ContractRelationOpInitialFields fields,ContractRelationOpInitialWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(host.frame) requires nonzero(append);

extern "C" ReadStatus contract_read_relation_op_initial(address context,ContractRelationOpInitialFields fields,ContractRelationOpInitialReader host) on cpu requires nonzero(host.origin) requires nonzero(host.frame);

extern "C" void contract_write_relation_edge(address output,ContractRelationEdgeFields fields,ContractRelationEdgeWriter host) on cpu requires nonzero(host.from) requires nonzero(host.op) requires nonzero(host.when);

extern "C" ReadStatus contract_read_relation_edge(address context,ContractRelationEdgeFields fields,ContractRelationEdgeReader host) on cpu requires nonzero(host.from) requires nonzero(host.op) requires nonzero(host.when);

extern "C" void contract_write_relation_node(address output,ContractRelationNodeFields fields,ContractRelationNodeWriter host) on cpu requires nonzero(host.seeds) requires nonzero(host.edges);

extern "C" ReadStatus contract_read_relation_node(address context,ContractRelationNodeFields fields,ContractRelationNodeReader host) on cpu requires nonzero(host.seeds) requires nonzero(host.edges);

extern "C" void contract_write_relation_write(address output,ContractRelationWriteFields fields,ContractRelationWriteWriter host) on cpu requires nonzero(host.target) requires nonzero(host.fields) requires nonzero(host.when);

extern "C" ReadStatus contract_read_relation_write(address context,ContractRelationWriteFields fields,ContractRelationWriteReader host) on cpu requires nonzero(host.target) requires nonzero(host.fields) requires nonzero(host.when);

extern "C" void contract_write_relation_frame(address output,ContractRelationFrameFields fields,ContractRelationFrameWriter host) on cpu requires nonzero(host.parent) requires nonzero(host.writes);

extern "C" ReadStatus contract_read_relation_frame(address context,ContractRelationFrameFields fields,ContractRelationFrameReader host) on cpu requires nonzero(host.parent) requires nonzero(host.writes);

extern "C" void contract_write_relation_graph(address output,ContractRelationGraphFields fields,ContractRelationGraphWriter host) on cpu requires nonzero(host.nodes) requires nonzero(host.variables) requires nonzero(host.frames);

extern "C" ReadStatus contract_read_relation_graph(address context,ContractRelationGraphFields fields,ContractRelationGraphReader host) on cpu requires nonzero(host.nodes) requires nonzero(host.variables) requires nonzero(host.frames);

extern "C" void contract_write_ownership_owned(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_ownership_borrowed(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_ownership_moved(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_address_identity_constant(address output,ContractAddressIdentityConstantFields fields,ContractAddressIdentityConstantWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.bits) requires nonzero(append);

extern "C" ReadStatus contract_read_address_identity_constant(address context,ContractAddressIdentityConstantFields fields,ContractAddressIdentityConstantReader host) on cpu requires nonzero(host.bits);

extern "C" void contract_write_address_identity_value(address output,ContractAddressIdentityValueFields fields,ContractAddressIdentityValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.ids) requires nonzero(append);

extern "C" ReadStatus contract_read_address_identity_value(address context,ContractAddressIdentityValueFields fields,ContractAddressIdentityValueReader host) on cpu requires nonzero(host.ids);

extern "C" void contract_write_address_identity_storage(address output,ContractAddressIdentityStorageFields fields,ContractAddressIdentityStorageWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(append);

extern "C" ReadStatus contract_read_address_identity_storage(address context,ContractAddressIdentityStorageFields fields,ContractAddressIdentityStorageReader host) on cpu requires nonzero(host.borrow);

extern "C" void contract_write_address_origin(address output,ContractAddressOriginFields fields,ContractAddressOriginWriter host) on cpu requires nonzero(host.root) requires nonzero(host.layout) requires nonzero(host.base) requires nonzero(host.words) requires nonzero(host.writable) requires nonzero(host.initialized);

extern "C" ReadStatus contract_read_address_origin(address context,ContractAddressOriginFields fields,ContractAddressOriginReader host) on cpu requires nonzero(host.root) requires nonzero(host.layout) requires nonzero(host.base) requires nonzero(host.words) requires nonzero(host.writable) requires nonzero(host.initialized);

extern "C" void contract_write_borrow_source_data(address output,ContractBorrowSourceDataFields fields,ContractBorrowSourceDataWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.id) requires nonzero(append);

extern "C" ReadStatus contract_read_borrow_source_data(address context,ContractBorrowSourceDataFields fields,ContractBorrowSourceDataReader host) on cpu requires nonzero(host.id);

extern "C" void contract_write_borrow_source_parameter(address output,ContractBorrowSourceParameterFields fields,ContractBorrowSourceParameterWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.id) requires nonzero(host.indices) requires nonzero(append);

extern "C" ReadStatus contract_read_borrow_source_parameter(address context,ContractBorrowSourceParameterFields fields,ContractBorrowSourceParameterReader host) on cpu requires nonzero(host.id) requires nonzero(host.indices);

extern "C" void contract_write_borrow_source_unresolved(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_borrow_source_inactive(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_borrow_source_external(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append);

extern "C" void contract_write_borrow_source_position(address output,ContractBorrowSourcePositionFields fields,ContractBorrowSourcePositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(append);

extern "C" ReadStatus contract_read_borrow_source_position(address context,ContractBorrowSourcePositionFields fields,ContractBorrowSourcePositionReader host) on cpu requires nonzero(host.origin);

extern "C" void contract_write_borrow_source_element(address output,ContractBorrowSourceElementFields fields,ContractBorrowSourceElementWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parent) requires nonzero(host.fields) requires nonzero(host.index) requires nonzero(append);

extern "C" ReadStatus contract_read_borrow_source_element(address context,ContractBorrowSourceElementFields fields,ContractBorrowSourceElementReader host) on cpu requires nonzero(host.parent) requires nonzero(host.fields) requires nonzero(host.index);

extern "C" void contract_write_borrow_source_projection(address output,ContractBorrowSourceProjectionFields fields,ContractBorrowSourceProjectionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parent) requires nonzero(host.fields) requires nonzero(append);

extern "C" ReadStatus contract_read_borrow_source_projection(address context,ContractBorrowSourceProjectionFields fields,ContractBorrowSourceProjectionReader host) on cpu requires nonzero(host.parent) requires nonzero(host.fields);

extern "C" void contract_write_borrow_source_contents(address output,ContractBorrowSourceContentsFields fields,ContractBorrowSourceContentsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(host.word) requires nonzero(append);

extern "C" ReadStatus contract_read_borrow_source_contents(address context,ContractBorrowSourceContentsFields fields,ContractBorrowSourceContentsReader host) on cpu requires nonzero(host.origin) requires nonzero(host.word);

extern "C" void contract_write_borrow_source_relation(address output,ContractBorrowSourceRelationFields fields,ContractBorrowSourceRelationWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.graph) requires nonzero(append);

extern "C" ReadStatus contract_read_borrow_source_relation(address context,ContractBorrowSourceRelationFields fields,ContractBorrowSourceRelationReader host) on cpu requires nonzero(host.graph);

extern "C" void contract_write_borrow_source_task(address output,ContractBorrowSourceTaskFields fields,ContractBorrowSourceTaskWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.captures) requires nonzero(host.outputs) requires nonzero(host.initialized) requires nonzero(host.guards) requires nonzero(host.completions) requires nonzero(append);

extern "C" ReadStatus contract_read_borrow_source_task(address context,ContractBorrowSourceTaskFields fields,ContractBorrowSourceTaskReader host) on cpu requires nonzero(host.captures) requires nonzero(host.outputs) requires nonzero(host.initialized) requires nonzero(host.guards) requires nonzero(host.completions);

extern "C" void contract_write_borrow_source_task_output(address output,ContractBorrowSourceTaskOutputFields fields,ContractBorrowSourceTaskOutputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.task) requires nonzero(host.projection) requires nonzero(append);

extern "C" ReadStatus contract_read_borrow_source_task_output(address context,ContractBorrowSourceTaskOutputFields fields,ContractBorrowSourceTaskOutputReader host) on cpu requires nonzero(host.task) requires nonzero(host.projection);

extern "C" void contract_write_residency_contract(address output,ContractResidencyContractFields fields,ContractResidencyContractWriter host) on cpu requires nonzero(host.required) requires nonzero(host.calls);

extern "C" ReadStatus contract_read_residency_contract(address context,ContractResidencyContractFields fields,ContractResidencyContractReader host) on cpu requires nonzero(host.required) requires nonzero(host.calls);

extern "C" void contract_write_residency_call(address output,ContractResidencyCallFields fields,ContractResidencyCallWriter host) on cpu requires nonzero(host.function) requires nonzero(host.arguments) requires nonzero(host.bindings) requires nonzero(host.path);

extern "C" ReadStatus contract_read_residency_call(address context,ContractResidencyCallFields fields,ContractResidencyCallReader host) on cpu requires nonzero(host.function) requires nonzero(host.arguments) requires nonzero(host.bindings) requires nonzero(host.path);

extern "C" void contract_write_ordinary_contract(address output,ContractOrdinaryContractFields fields,ContractOrdinaryContractWriter host) on cpu requires nonzero(host.parameters) requires nonzero(host.result) requires nonzero(host.inputs) requires nonzero(host.element_inputs) requires nonzero(host.returned) requires nonzero(host.written) requires nonzero(host.requires) requires nonzero(host.target) requires nonzero(host.frame) requires nonzero(host.fallible) requires nonzero(host.domain) requires nonzero(host.residency) requires nonzero(host.view_access) requires nonzero(host.initialization);

extern "C" ReadStatus contract_read_ordinary_contract(address context,ContractOrdinaryContractFields fields,ContractOrdinaryContractReader host) on cpu requires nonzero(host.parameters) requires nonzero(host.result) requires nonzero(host.inputs) requires nonzero(host.element_inputs) requires nonzero(host.returned) requires nonzero(host.written) requires nonzero(host.requires) requires nonzero(host.target) requires nonzero(host.frame) requires nonzero(host.fallible) requires nonzero(host.domain) requires nonzero(host.residency) requires nonzero(host.view_access) requires nonzero(host.initialization);

extern "C" void contract_write_proof(address output,ContractProofFields fields,ContractProofWriter host) on cpu requires nonzero(host.origins) requires nonzero(host.conditions) requires nonzero(host.ranges) requires nonzero(host.nonzero) requires nonzero(host.domains) requires nonzero(host.views) requires nonzero(host.addresses);

extern "C" ReadStatus contract_read_proof(address context,ContractProofFields fields,ContractProofReader host) on cpu requires nonzero(host.origins) requires nonzero(host.conditions) requires nonzero(host.ranges) requires nonzero(host.nonzero) requires nonzero(host.domains) requires nonzero(host.views) requires nonzero(host.addresses);

extern "C" ReadStatus contract_read_address_root(Bytes input,inout u64 position,address context,address output,ContractAddressRootVariants host) on cpu requires nonzero(host.local) requires nonzero(host.parameter) requires nonzero(host.resource) requires nonzero(host.buffer) requires nonzero(host.derived) requires nonzero(host.external) requires nonzero(host.dataset);

extern "C" ReadStatus contract_read_initialization_source(Bytes input,inout u64 position,address context,address output,ContractInitializationSourceVariants host) on cpu requires nonzero(host.position) requires nonzero(host.task);

extern "C" ReadStatus contract_read_element_facts(Bytes input,inout u64 position,address context,address output,ContractElementFactsVariants host) on cpu requires nonzero(host.unknown) requires nonzero(host.known) requires nonzero(host.cleanup) requires nonzero(host.common) requires nonzero(host.selected) requires nonzero(host.input);

extern "C" ReadStatus contract_read_data_layout(Bytes input,inout u64 position,address context,address output,ContractDataLayoutVariants host) on cpu requires nonzero(host.declared) requires nonzero(host.parameter);

extern "C" ReadStatus contract_read_returned(Bytes input,inout u64 position,address context,address output,ContractReturnedVariants host) on cpu requires nonzero(host.unit) requires nonzero(host.value) requires nonzero(host.dataset) requires nonzero(host.owned_value);

extern "C" ReadStatus contract_read_task_projection(Bytes input,inout u64 position,address context,address output,ContractTaskProjectionVariants host) on cpu requires nonzero(host.returned) requires nonzero(host.writes) requires nonzero(host.places) requires nonzero(host.initialized);

extern "C" ReadStatus contract_read_exclusion(Bytes input,inout u64 position,address context,address output,ContractExclusionVariants host) on cpu requires nonzero(host.resource) requires nonzero(host.ownership) requires nonzero(host.position) requires nonzero(host.view);

extern "C" ReadStatus contract_read_view_access(Bytes input,inout u64 position,address context,address output,ContractViewAccessVariants host) on cpu requires nonzero(host.resource) requires nonzero(host.position) requires nonzero(host.memory) requires nonzero(host.dataset) requires nonzero(host.initialized) requires nonzero(host.stored);

extern "C" ReadStatus contract_read_residency_part(Bytes input,inout u64 position,address context,address output,ContractResidencyPartVariants host) on cpu requires nonzero(host.field) requires nonzero(host.elements) requires nonzero(host.selected);

extern "C" ReadStatus contract_read_residency_origin(Bytes input,inout u64 position,address context,address output,ContractResidencyOriginVariants host) on cpu requires nonzero(host.cpu) requires nonzero(host.gpu) requires nonzero(host.input) requires nonzero(host.output) requires nonzero(host.task_transfer);

extern "C" ReadStatus contract_read_residency(Bytes input,inout u64 position,address context,address output,ContractResidencyVariants host) on cpu requires nonzero(host.absent) requires nonzero(host.origins) requires nonzero(host.selected);

extern "C" ReadStatus contract_read_residency_output_kind(Bytes input,inout u64 position,address context,address output,ContractResidencyOutputKindVariants host) on cpu requires nonzero(host.returned) requires nonzero(host.written);

extern "C" ReadStatus contract_read_relation_op(Bytes input,inout u64 position,address context,address output,ContractRelationOpVariants host) on cpu requires nonzero(host.identity) requires nonzero(host.position) requires nonzero(host.read) requires nonzero(host.element) requires nonzero(host.projection) requires nonzero(host.resources) requires nonzero(host.owners) requires nonzero(host.select) requires nonzero(host.initial);

extern "C" ReadStatus contract_read_ownership(Bytes input,inout u64 position,address context,address output,ContractOwnershipVariants host) on cpu requires nonzero(host.owned) requires nonzero(host.borrowed) requires nonzero(host.moved);

extern "C" ReadStatus contract_read_address_identity(Bytes input,inout u64 position,address context,address output,ContractAddressIdentityVariants host) on cpu requires nonzero(host.constant) requires nonzero(host.value) requires nonzero(host.storage);

extern "C" ReadStatus contract_read_borrow_source(Bytes input,inout u64 position,address context,address output,ContractBorrowSourceVariants host) on cpu requires nonzero(host.data) requires nonzero(host.parameter) requires nonzero(host.unresolved) requires nonzero(host.inactive) requires nonzero(host.external) requires nonzero(host.position) requires nonzero(host.element) requires nonzero(host.projection) requires nonzero(host.contents) requires nonzero(host.relation) requires nonzero(host.task) requires nonzero(host.task_output);
