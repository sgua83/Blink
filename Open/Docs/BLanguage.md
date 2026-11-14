# Blink 语言规范

Blink（简称 B）是一门自研原生并行计算语言。`.h` 声明模块接口，`.b` 提供实现，由 Rust 编写的编译器生成原生代码。

B也寓意Base。Blink希望像C语言为上一个计算时代所做的那样，成为AI超算时代的基础语言。本文描述当前已实现规则；软件版本见根VERSION，长期目标、实现、原生结果与性能分别记录。


编译、调试、观察和基础量化工具以AI为唯一直接使用者，架构师通过AI分析；长期支持AI用普通B函数编写观察／分析程序、在数据附近处理所选事实、表达明确机器方案并组合真实输入实验。原检查器与资源所有者约束访问、效果、ABI和最后完成，优化策略、调试信息与采集独立。这些长期方向尚未形成观察程序或机器方案的正式语法与执行命令；已交付的基础查询见下述对应规则。既有导出、栈报告、GPU计时与完成接口不代表这些设计已交付，实际支持以对应现行规则和原生结果为准。

B 不采用面向对象，不设计类继承、虚函数、对象类型驱动的运行时动态派发、通用类型擦除动态对象或反射驱动调用，以数据、函数、模块和执行域组织程序。普通B通过组合、编译期泛型与特化、模块接口复用，目的是尽量在编译期发现类型、调用、访问、副作用及生命周期问题，减少依赖运行时绑定和调用者约定的关系。必要动态性集中在真实外部ABI、模块装载／执行版本及资源调度边界；动态输入、容器、显式变体和普通任务仍属完整通用能力，不要求全部数值或调度预先固定。编译器可控的原生并行与调度、跨机一致语义和稳定热更新是长期核心目标，本规范不将这些目标描述为当前已实现能力。

B应具备独立编写完整系统程序的能力。正确的系统ABI声明，以及承担类型／编码／错误／资源管理的B系统库属于实际能力；已有Windows接口可以直接沿原生ABI调用系统DLL，后续其他64位平台仍须分别实现。独立且必要的原生支持可以保留Rust，B外壳或相同参数的重复转发不代表B接管。编译器主体自举、调试／观察主体实际接管、保留的Rust支持与完整工具链脱离Rust分别报告，不以百分之百B或源码比例判完成；具体边界统一见Architecture及NativeInterop，当前实现状态保持如实记录。

内存安全是核心目标。本文各节只说明已实现的类型、初始化、索引、借用、并行访问及显式外部边界规则；当前仍有具体未闭合能力和实现缺陷，不宣称完整内存安全或无泄漏已经交付。

## 确定的CPU运行目标：仅64位

BLanguage只面向64位CPU运行，不支持32位CPU，不建设32位CPU后端、运行库或ABI兼容路径。当前已实现的CPU目标是Windows x64；其他64位CPU／平台仍按各自后端、调用约定和运行库的实际实现记录。

CPU位数不决定普通值类型的位宽。u32、i32、f32等32位类型保持原语义，u8／i8／u16／i16及系统结构同样按真实布局定义；不把所有数据或GPU运算改为64位。u64／i64的数值能力见下节；address值及usize／isize按本节后的原生地址规则交接；地址的实际位置访问与回调仍有各自能力范围。CPU目标不等于已支持全部64位平台。

## 原生地址值与指针宽整数

address表示本进程CPU地址空间中的原始字节地址，大小与对齐均为8字节，一个U64逻辑分量；它是独立类型，不是u64、handle或view。地址值不持有载荷、长度、权限或释放函数；复制／传参／返回只交接同一地址，不移动其指向存储，不延长实际所有者和最后消费者的寿命。空地址的位模式为零，address()构造空值，address(existing)只复制同型身份。地址相等／不等比较保留全部64位；其他数值运算先显式使用地址位模式，不将address当作numeric类型族或可直接索引资源。

usize与isize分别是u64与i64的同义类型，固定为64位，仅用于明确指针宽度的计数和偏移意图；与uint／u32的既有同义规则一致，不生成另一份数值实现，不建设32位CPU支线，也不改变dataset下标及const形状的既有类型。

CPU位置的unsafe_address(usize_value)将完整64位整数位模式交给address，address_bits(address_value)取回同一usize位模式；二者不访问存储。address及记录／固定数组中地址字段的静态来源沿原Proof／Borrow保留，普通B实参、返回和输出合同交接同一来源；它不是资源拥有者。循环和自调用中重复读取地址字段沿同一有限读点、入口及回边关系交接，实际读点保留此前写入，已求值地址仍是自己的读取快照；原值转交的递归地址结果复用该入口关系。调用方按本次实参与调用前内容检查真实布局、读写、初始化、寿命和内存别名，源、obj及lib使用同一合同。私有LoopInput和局部槽不进入公开合同，已结束私有读取取得的公开来源继续由原关系交接。常量地址、unsafe_address、function_address及C返回／输出和已明确External的位置读取由其真实生产入口明确标为External，表示该值未携带Blink数据所有者来源；这一标记本身不证明实际地址不相交，不把未知来源或未活动变体视为外部地址。address_bits只交出整数位模式，其数值结果不继续携带地址借用；重新unsafe_address属于新的显式外部解释。符号address参数所指聚合中的地址字段读取，以该次实际读取的输入存储内容形成来源合同；调用处消费原操作前的位置快照，已求值地址保持读取时的来源，新读取沿此前真实写入交接。同一位置的完整覆盖、可能别名、分支及真实返回出口分别处理；普通B调用及obj／lib保留同一内容来源和最终写回，不因合同已闭合跳过调用方的位置更新。返回出口拒绝仍留在调用方存储中的当前函数局部地址，退出前已经完整清除的临时存放保持合法。位置本身的来源不等于其中所存地址值的来源；已知本地内容保留真实所有者，明确External由原受信任入口生产，未知来源不改标External，不绕过布局、权限或寿命。此范围实施见FixedIssues C-005.90，正式种子采用分别记录，不等于全部裸地址别名或完整内存安全已实现。编译期在原Proof中按实际值分量交接可证明的地址位模式与原存储／字节区间关系；它与静态借用分开，不授予权限、初始化或寿命。复制、记录／固定数组、条件选择及普通B参数／返回／输出保留同一数值来源；同宽64位整数转换保持位模式，缩窄或改变位模式的运算撤去该关系。真实写入沿原数值地址来源和字节范围投影到原Local或资源位置；已知资源没有局部槽位不构成来源未知。原资源与其字段字偏移交给同一数值失效消费者，真正写入必须撤回相应旧事实，未写字段及独立资源保持；原范围无法闭合、越出已知存储或真实未知地址仍按External处理。公开对象合同只用原输入重绑定，不导出被调私有Local／slot，整数结果不延长原存储寿命。同一普通CPU函数自身局部存储的直接位置读写支持这种整数往返：External可能与原Local相交，按原局部表形成实际IR别名，读取保留必要存储，写入使可能受影响的原数值及地址来源事实失效；记录／固定数组和控制流使用同一规则。整数没有恢复原静态借用，未知写入不证明某一原位置已初始化。不透明的命名及间接C调用交接原External内存效果，同一函数内显式取址的局部存储在同步回调经整数userdata读写后，普通读取、字段／固定数组及循环按实际内存与失效后的数值事实继续执行。拥有资源的私有描述符不属于普通C可写值；已有实际拥有模型时，其句柄和长度继续由原资源操作交接，立即不透明写与延后Memory捕获沿同一资源字节范围排除这些分量。普通字段、载荷及真正未知地址的写仍使受影响事实失效，显式已知位置写继续按实际覆盖处理。未知写入保留原来仍可能有效的地址借用并加入External来源，不证明确定覆盖或初始化，不改变调用前已取出的普通值快照。普通B函数及其obj／lib以原Access合同交接资源互斥、真实类型位置访问及不透明C内存效果，地址形成及B实参暴露不冒充被调实际访问；原调用连接自身MemoryTarget，在失败清理等实际闭合消费者结束前重填Call.memory，原模块最后交接才结束该私有编号，按实际Position的读写操作保持Read／Write／InOut，不叠加实参地址能力或把只读提升为写入；External范围保留同一方向，真正混合读写完整合流。跨函数整数地址访问保留必要物理存储及别名，never调用也保留真实读取。整数仍不恢复静态借用或确定初始化；已有闭合访问合同的普通B调用按实际条件只使真正写入位置及不透明C可能影响的数值事实失效，纯地址值、只读及已取出的SSA快照保持；源函数完成时沿原调用分量闭合访问合同并交接，尚未闭合与合法空合同分别表示；循环对已有闭合B合同复用普通调用的Access、已形成输入快照和when，只撤回真实可能写位置，未写局部、记录字段与固定数组分量保持原事实；声明的write／inout权限不直接成为实际写。位置投影和普通写入共用原位置分量查询，不再次求值实参；当前未生成的动态来源仍为未知，不能冒称无写。循环扫描先通过原native_declaration形成或导入所需的同一词法实例合同，再消费入口条件与正文读取；Module保持实例只构造一次，不提前执行实参或构造无关函数。普通CPU调用的访问合同或本次真实写来源尚未闭合时，原Function.uses中的Memory记录持有操作前实际位置的数值快照，原绑定只交接该操作的MemoryValue；普通B调用、C调用及原生位置写入共用此接点，Call只拥有被调关系和实参，不再单独拥有数值快照。形成或解释地址只交接位置能力，真实read_place／write_place生产实际读写方向，不预先把位置解释记作InOut。后续读取仍发射真实Load，不提前复用操作前常量、范围或非零许可。原Module先闭合实际Access及原循环来源，再沿原Memory使用顺序，把已经解析的前次MemoryValue代入本次访问条件及来源，形成当前MemoryEffect后交付本次快照；同一次C调用的原Memory使用共同消费其实际操作，后次操作消费已有前缀，不另建调用、图或数值表。当前快照不能用于证明当前操作无写；普通位置与资源共同普通载荷使用同一Memory.snapshots／MemoryValue，不可变快照随原使用记录共享，实际Origin、条件及字节布局保留到原Position消费者，IR出口再投影Local／Data别名。普通拥有聚合与资源共同模型在同一个快照捕获入口按实际活动字段排除资源描述分量；资源描述及子长度继续归原类型化拥有模型，真实write／inout交接实际新资源，空变体不产生子资源；原绑定表保留同一Module的实际延后数值身份，快照形成后按同一值捕获其条件，由原分支合流消费。未写位置继承原事实，可能写位置保持未知，已捕获SSA值独立。普通Local数值快照消费原Origin的实际字节位置及已有Presence选择，静态／动态候选只撤回相交分量。普通资源元素的数值位置同时保留实际资源、原元素布局、字段字节及本次已求值下标，初始化标记不代替这份存储坐标；没有待初始化槽仍须处理真实写入。当前数值绑定、循环未来写和原Memory数值快照使用同一位置查询；已证明同一资源内不同元素的写不撤回该读取，可能相交及未知写继续撤回，可能写不表示必写。共同元素模型继续表达全部元素共同成立的事实，不取得某个下标的排除许可。快照下标按原已解析输入代入，只属于原编译Module；地址载荷的实际内容、读取时点及完整覆盖继续各按原内容合同。IR整块Local别名仍保留真实物理读写，不作为数值写范围。无法闭合的原位置及未知写继续保守。类型化引用在原Address／PositionAccess分别交接可写能力与实际写入：实参准备只检查权限，真实赋值沿原Parameter／Derived字段投影进入写效果；普通与拥有引用共用同一原参数位置来源，未写字段事实保持。只读、布局与初始化合同仍须满足。递归构造函数的循环入口沿同一已闭合访问合同交接；部分字节覆盖后的数值事实按实际覆盖字节、原Proof位事实和当前Clause消费，真实重叠写仍撤回旧身份。此时需要等待的原整数除数要求保存在同一Function使用记录，继续消费原非零规则，不另建数值图、SSA解释器或验证器。返回／写回和访问合同沿原规范输入／输出身份交接解析后的数值事实；这些快照只属于编译Module，不进入运行时资源或ABI。同一已解析数值事实也进入原初始化及调用访问查询的路径上下文。原typed Reference写、初始化、借用与最后资源完成继续由原所有者消费。原跨B内存效果与循环预分析已完成，见C-020.2.7.2.3.2.2；整数往返及同步C整数userdata对普通CPU局部、固定数组与dataset载荷的原存储交接已完成，见原地址与同步C边界。可保留回调的代码／上下文寿命继续归C-021.6.1.3。公开暴露、权限及存活期继续沿真实消费者完善，不将当前范围写成永久语言限制。形成地址的调用者负责实际来源、空值用途、指向布局与对齐、允许读写范围及存活期；非零、对齐数字或某次比较本身不能证明地址可访问。不能通过普通u64／bool／浮点构造、bitcast或handle转换绕开这一次显式地址解释。系统API返回的地址按其真实合同使用；例如GetCommandLineW的存储由系统管理，应用不能修改或释放。

实际元素读取若依赖尚未闭合的MemoryValue，由本次既有Dataset读取记录持有原元素Proof及同一次已求值下标／资源绑定；原uses顺序先解析此前Memory值，再投影本次实际选择，交接本次读取自己的数值身份。所有可能的元素模型共同合流，不能只保留待解候选。它不追加旧调用快照，不重走调用链；真实或未知相交写撤回不再共同成立的旧事实，读取前已捕获的普通SSA值保持独立。 实际赋值的可能写入数值沿原位置合同交接。可定位且同标量完整覆盖时，原共同数值更新合流旧值与全部可能写入值，只保留共同成立的范围、非零或Bool条件；原值与所有可能写入值均为false时，新读取仍可得false。可能写入不表示必写或最终确定值，不授予初始化、权限、借用或寿命，也不保留旧完整值身份。未知写、不能按同标量完整对应的写及部分字节写继续按原实际覆盖撤回受影响事实并保留仍有效的未写位。直接写、闭合调用、延后MemoryValue及无提供者源码的obj／lib使用同一合同与原数值更新，不增加快照、数值表、内存图或运行时状态。整数局部或dataset元素只被部分字节覆盖时，编译器按原Origin的实际写区撤去旧完整值身份、地址和条件，在同一Proof.domains保留未写整数位；重复覆盖、截断及扩宽沿实际类型位语义处理。直接写、闭合调用和延后Memory读取共用这一字节关系，共同元素模型仍合流未写元素及可能新值。同一循环资源沿相同字段／元素投影得到的存储保持原布局和字节范围，不以来源或寿命已经闭合作为字节查询前提；普通字段写不失效未覆盖的资源描述符。真实元素选择、祖先下标、不同来源及未知别名继续由原位置合同交接。完整覆盖与未知写不继承旧位事实，位事实不授予初始化、借用或资源寿命。当前按Windows x64布局实现，独立obj／lib以对象格式交接；条件赋值沿同一Clause保存两侧关系，原共同事实查询保留共同整数位；后继部分写消费当时路径及此前已解快照，再撤去真实写字节。最终非零查询使用原数值上下文，obj／lib沿同一合同消费；整数转换沿原CommonRange保留来源类型、截断、扩展及目标类型；实际非零查询由同一scoped_proof读取原环境和本次路径中的来源位事实，按这条真实转换链投影到目标位宽。已知位与范围继续属于原Proof.domains，符号扩展沿实际符号位处理；目标可能为零或来源未知时不取得新的非零事实，不恢复旧值身份、地址、初始化或资源寿命。

address可作为普通值、记录／固定数组／变体字段及普通函数参数／结果，已编译模块保留其类型身份、大小、对齐及公开合同。Windows x64外部C调用按真实8字节POINTER值及原寄存器／栈参数布局交接，read／write地址位置按已有连续位置ABI交接其存储地址。地址数值与所指对象对齐分别判断：一个地址字段自身按8字节对齐，不因此声称所指对象已满足布局。平台依据见[Windows x64 ABI](https://learn.microsoft.com/en-us/cpp/build/x64-software-conventions)。

### 外部地址的显式CPU位置

unsafe_position<T>(address_value)在CPU执行位置解释一份真实外部地址，得到T的实际位置表达式；按值使用时读取此刻载荷，赋值、字段／固定数组投影及普通B／C的read、write、inout参数均沿原位置寻址，不先复制到临时值再写回。T使用现有非空普通值布局，支持标量、向量、矩阵、记录、固定数组及普通enum／result；不能包含dataset、view、纹理、加速结构、owned或其他Blink拥有资源。

调用者承担这一unsafe入口的外部存储合同：地址来自有效的本进程CPU存储，覆盖T的真实大小并满足T对齐；本次使用具备相应读写权，读取及inout所需逻辑字段已经初始化且是合法T值；实际存储在最后一个同步读写／借用消费者完成前存活。外部存储不因该位置进入Frame拥有树，作用域结束不会隐式释放它；普通read／write／inout借用只在实际调用期间有效，仍不得逃逸。该入口本身不读取载荷，因而write可初始化原先未初始化的外部存储。

原始地址访问不能与仍活跃的B位置借用冲突；调用者负责编译器无法从外部地址恢复的别名关系，不得经另一个原始地址修改正在被B函数read／inout借用的位置。编译器沿原参数借用检查拒绝已知同址、复制地址及部分字节重叠的冲突，按真实标量宽度判断范围；不同未知地址不自动证明不相交。已知来源的布局、权限、初始化及寿命检查不构成全部裸地址别名的证明；当前不会穷尽诊断跨语句裸地址写入与待执行静态F之间的读取冲突，显式unsafe位置访问仍须由调用者遵守原借用合同。提供安全B接口的库实现必须在其真实unsafe边界承担这项责任，不能把编译器接受当作别名合法性证明。unsafe入口不增加空值／范围探测、运行时登记、锁或复制。它不授予B自有局部／资源的取址或保留借用权限，不能把违反外部C借用寿命所得的地址当作合法外部来源。

例如unsafe_position<Pair>(pointer).left直接访问原记录字段；修改函数change(inout Pair value)接收unsafe_position<Pair>(pointer)时借用同一原位置。array<u32,4>等固定数组保留声明步长及原下标规则，读取复制出的普通值是独立快照，不成为对原存储的长期引用。

address_of(place)取得现有连续CPU普通位置的真实地址，不先读取或复制载荷。取址本身不改写载荷，循环中仅形成地址也不撤回该位置仍成立的数值事实；真实写入继续按原位置与调用效果处理。局部变量、字段、固定数组元素及满足原连续布局合同的位置参数共用原位置；CPU资源元素保留原资源来源。地址沿原Proof／借用合同保存位置布局、只读／可写权限和初始化来源，unsafe_position<T>不能改变已知原布局或升级只读权限；读取仍须原位置已初始化。普通值、记录／固定数组、B函数返回及独立对象／静态库必须交接同一实际地址载荷来源；位置本身的来源不代替其中地址值的来源。写入效果使实际受影响位置及CPU dataset元素公共模型中的旧数值事实失效。资源字段地址保留原元素布局和实际字段字节偏移；同一原写入入口沿已知资源及字节分量撤回旧公共数值，不将字段位置当成整个独立局部值的零偏移，不改资源身份、初始化、借用及最后释放。共同元素模型失去某字段的普遍数值保证，与已求值且之后没有相交写入的装载快照分别交接；已经证明不相交的写保留原快照，实际或未知相交写仍撤回对应数值关系。当前函数的局部存储或将释放资源不能作为地址结果逃出；仅移动本地拥有元数据、保持实际存储和资源身份的操作沿原规则允许，真实存储迁移／释放不得越过仍活跃消费者。

地址按值读取形成当时的载荷快照，后续写入不改变已经取出的值；下一次读取须使用实际覆盖、分支及退出之后的内容。通过普通B函数写入地址字段仍须交接同一载荷来源，不能用位置来源、数值失效或External标签替代；局部地址在原存储寿命内暂存并在逃出前清除，与局部地址在函数结束后仍被调用方持有分别处理。符号输入记录中读出的address是该次读取的目标地址值，承载字段的存储位置与所指位置分别交接。目标仍只由外部输入提供时按原显式地址合同解释；真实调用代入已知B位置时，继续按其实际布局、读写权限和初始化状态检查。源、obj及lib复用原读取时点与实际内容，不把未知或局部地址改成External，不从字段自身的布局推断其目标。

普通B调用、条件分支、提前返回及独立对象／静态库已交接通过address确定写入的原分量与成立条件；函数布尔结果可供调用方在对应分支读取已初始化位置。字段只要求其真实分量，不能由左字段写入推定右字段完成；选择地址只初始化实际所选位置，不能把全部候选标成完成。私有分支可能漏写时不构成无条件保证；全部正常返回均已写入时，调用方无需再次证明被调内部的私有出口条件。自递归／互递归沿实际写入位置及原正常退出的写入／调用条件归纳确定结果，保留条件、字段及参数置换；读取与调用前提只消费该时点已有事实，不能由后续写入补足。现有退出条件不能闭合的递归数值关系不推定完成。已闭合B调用的确定写入交接原位置，可用于固定数组及CPU write dataset的完整for初始化，字段、continue与提前break保持原覆盖规则；递归写入的完整for覆盖、循环后的实际读取／转借，以及被调函数内完整循环形成的正常返回保证沿同一合同交接；提前break、跳过元素和未写字段仍不能当作完整初始化；普通位置及dataset字段的条件输出查询也在原guard下保留该时点的Position／原uses，闭合后按同一声明判断，false及漏写不获初始化。取址和可能写入都不等于初始化完成。拥有记录、拥有元素序列可同时返回CPU资源及指向其实际存储的地址，普通返回／转交和拥有字段移动保持原资源身份；原局部或没有相应拥有者的地址仍不能逃逸。own的静态释放F所保存地址计入原最后消费者，在其结束前不得resize、迁移或释放实际存储；最后读取之后原拥有者可以继续resize。该能力不表示任意借用返回、异步外部回调或跨设备地址已实现。Windows x64系统信息的具体B接口、实际结构／地址输出与原生消费范围见[blink.windows.system](../../library/README.md#blinkwindowssystem)；高精度计数与频率以[blink.windows.time](../../library/README.md#blinkwindowstime)的i32状态／条件write i64直接交接，分别保留ticks与ticks／秒，不代表日历时间或GPU计时；[blink.windows.error](../../library/README.md#blinkwindowserror)直接交接原OS线程的32位错误码读取／设置，错误快照由调用者立即保存，不自动清零或转换业务结果；[blink.windows.process](../../library/README.md#blinkwindowsprocess)以纯B同步读取OS原命令行并交回独立拥有dataset<u16>，不含NUL、不替换UTF-16代码单元；同模块parse_command_line／arguments以B按Windows规则解析为拥有的Arguments{units,ranges}，保留空参数和实际代码单元，argv[0]单独处理；原系统字符串不得修改／释放，输出按原Frame拥有结果交接。[blink.windows.environment](../../library/README.md#blinkwindowsenvironment)直接交接原只读UTF-16环境快照地址和i32释放状态；调用者在最后读取完成后成对释放原OS块，它不由Frame自动回收。同模块variable(address name)由B直接调用原GetEnvironmentVariableW，交回不含NUL的唯一拥有dataset<u16>；未定义203与空值区分，其他原OS错误及实际容量不足分别交接。名称在同步调用期间由调用者保持为真实NUL结尾UTF-16；长度查询后只读取一次，不返回未定义缓冲、重试或宣称原子快照。同模块SetEnvironmentVariableW(address name,address value)直接交接原i32 BOOL，失败后由调用方立即取原GetLastError；零value地址删除，非零NUL空串地址设置空值。OS在同步调用期间复制输入，返回后原名称／值仍归调用方；旧快照及已有B拥有结果不被回写，新子进程按实际创建参数继承环境，不改变其他既有进程或注册表。process模块直接声明CreateProcessW、GetExitCodeProcess；共用WaitForSingleObject、CloseHandle及等待／运行状态常量由纯.h blink.windows.system唯一声明，调用者显式导入system，基础StartupInfo／ProcessInformation沿真实Windows x64位置布局交接。创建和退出查询仅在原BOOL非零时初始化输出；原等待状态、完整u32退出码与即时GetLastError分别保留。成功句柄交调用方持有到最后实际等待／查询后各关闭一次，关闭不表示终止，创建不表示完成；明确接口和环境／目录／命令行寿命见[原生子进程](../../library/README.md#windows原生子进程)。blink.windows.process_extended以完整read StartupInfoEx提供限定名的同一原生CreateProcessW，并直接交接属性列表的字节查询、初始化、修改及删除；实际属性值存储保持到Delete，列表缓冲仍归原分配者。基础模块SetHandleInformation修改原句柄继承位；选择性继承以原真实可继承句柄数组、usize字节数及inherit_handles=1交给OS，不建立另一份句柄拥有权或隐式取消。原始address不能静态保证外部API保留的全部寿命，调用方按真实合同保持列表、值及句柄。编译器原Rust进程生产者的B采用仍待实现。当前进程的标准输入／输出／错误由[GetStdHandle](../../library/README.md#当前进程的标准句柄)直接借用原handle，复用ReadFile／WriteFile交接字节与原OS结果；选择值的u32和返回身份的64位分别保留，NULL缺失与INVALID_HANDLE_VALUE失败分别解释。借用不建立关闭责任，不推断端点编码或替代实际I/O完成。[blink.windows.thread](../../library/README.md#blinkwindowsthread)直接声明CreateThread、当前／指定线程ID及条件write u32的GetExitCodeThread；B C入口使用非零cfn<u32,address>，创建交回句柄不表示完成。原parameter存储和代码保持到实际线程结束，等待／关闭复用system，退出码完整保留；原始address不自动建立跨任务typed借用，普通B任务及异步借用仍按C-020.4.1建设。[Windows工作线程处理器组绑定](../../library/README.md#windows工作线程处理器组绑定)直接交接当前线程伪句柄、组内掩码、可选旧设置及原BOOL／OS错误。实际CPU由工作调用者从已有许可选择，真实线程完成仍由原等待交接。[Windows原生独占锁](../../library/README.md#windows原生独占锁)直接交接SRWLock原存储布局、完整write初始化及原地址获取／释放；使用中不移动，原持有线程释放，存储保持到最后实际消费者，锁不成为B资源拥有者或自动借用。[Windows编译许可条件等待](../../library/README.md#windows编译许可条件等待)在已有独占SRW锁上交接原子释放、等待、重新取得及唤醒。可用许可由原锁内谓词交接，超时保留真实OS错误。其他系统库仍按真实API逐项建设；C函数地址签名值及实际调用见下节；系统回调及最后消费者寿命归C-021.6.1的剩余叶项。设备地址、GPU／远端资源和本地CPU地址仍由各自真实存储所有者交接，本地地址位模式不授予跨设备或跨机访问能力。

算法DLL的CPU／GPU后端使用编译阶段已生成的原生映像，和应用包装共用resource编码；生成的Rust包装只嵌入映像字节，不重建类型、执行计划和阶段对象源码。映像保留同一入口类别、目标、版本与真实代码合同，实际装入、调用及释放仍归原运行库和宿主。

Windows x64已选CPU现场的单帧推进由[blink.windows.unwind](../../library/README.md#blinkwindowsunwind)以原C ABI直接交给DbgHelp.dll的StackWalk64。调用者保持真实进程／线程、原栈、原生展开记录、可修改CONTEXT和同一StackFrame64游标；首次成功可能仍为初始帧，后续按原更新结果逐帧使用，原零返回不伪造成完整栈尾或GetLastError中的确定原因。四个cfn回调按原SDK可空签名交接，具名原生查询通过c_function传入；符号处理资源沿相同进程标识初始化、使用及清理。数据及代码的最后消费者、目标停止／恢复与DbgHelp调用顺序仍归原所有者，库不新增转发实现、锁、调度器或状态表。实际寄存器选择、源码关联和完整回溯程序与该原生平台接口分别提供。

## 原生C函数地址的签名值

cfn<R,P...>表示当前Windows x64 C ABI的一份函数地址值，大小8字节、对齐8字节，一个U64物理分量。R为普通C值、void或never；P为普通按值类型，或read<T>、write<T>、inout<T>位置参数。T沿原普通非空C布局，不包含Blink拥有资源或view，也不接受atomic参数。返回类型、参数顺序、类型和访问方式共同组成签名身份；同宽但签名不同的函数值不是同一类型。

这是原生C ABI互操作能力。系统函数表和回调可以要求动态地址，普通B算法仍优先使用具名函数、静态引用、泛型及特化。签名值可存放、传参和返回，不因此取得代码所有权；签名、访问合同和非零检查也不证明外部实现或模块寿命。不另设“是否用于FFI”的用途白名单，也不把cfn扩展为普通B的通用动态函数对象或类型擦除闭包。

cfn<u32>()构造空函数地址；cfn<R,...>(existing)只复制相同完整签名值。相等／不等比较保留完整64位，仅允许同签名比较；空值可以保存、复制和传入准确的可空外部接口。函数值不是numeric、address或handle，不能数值运算、次序比较、bitcast或普通数值构造。

CPU中的unsafe_function<cfn<R,...>>(address_value)显式解释原地址的C签名，function_address(function_value)取得同一address身份；二者不调用代码，不装载或拥有DLL，也不延长目标代码寿命。unsafe_function产生cfn代码身份，不把原address的数据借用带入函数值；function_address在原地址值来源图中明确交接External。非空地址的真实导出、调用约定、完整签名及代码装载和最后消费者前存活由调用者保证；编译器不读取DLL机器签名。空地址在这里只是空值，不因签名解释获得调用许可。

函数签名保存原TypeName中已绑定的名义类型引用，实际cfn本体仅保存8字节地址，签名参数的read／write／inout只作为类型元数据；普通函数参数、结果、记录、固定数组、泛型及已编译B模块保持同一名义身份和8字节物理载荷。普通C ABI按实际函数指针值传递与返回；生成的通用算法数据载荷仍按物理U64交接，签名以B接口为准，不能把任意数值载荷当作真实可调用代码。复制函数地址不复制代码或外部模块所有者。

函数签名允许引用正在定义的记录、相互引用的记录，以及泛型／const实例；返回或按值参数引用记录也不等于把该记录嵌入函数地址。World完成真实记录布局后检查签名，调用沿原名义引用取得完整布局、字段与访问合同；直接按值包含自身的记录仍非法，拥有资源、view、空普通C值及非法位置签名继续拒绝。CPU函数值的实际间接调用已实现；明确C ABI的B定义或from外部声明可通过c_function形成无隐藏Frame的原生函数地址，见下节；需要Frame的资源型C入口仍未开放；可保留代码／状态的最后消费者寿命仍沿C-021.6.1.3建设。集合算法的静态函数引用、bind及owned<T,F>静态释放身份继续使用各自原机制，不因此变成运行期C函数值或已交付动态回调。

调用合同为CPU表达式value(args...)，目标可以是变量、字段、数组元素或返回的函数值；按源码顺序先求值并保存目标，再求实参，每个表达式只求值一次。调用要求原Proof已经证明目标非零，可通过与同签名空值比较的控制流或requires nonzero参数合同交接；不会插入运行时空值分支。非零不是目标签名、有效代码或寿命的证明，unsafe函数地址来源仍承担对应外部责任。

Result及一般带载荷变体保留活动载荷的完整整数／函数地址非零关系。64位值拆成两个32位分量时，只表达至少一个分量非零，不要求或推断每个半字非零；match／try沿实际标签选择载荷后，重组值消费同一事实。未知、零值、其他分支及改写后的内容不继承原非零许可。

签名子结构中的const实参可沿原泛型匹配器，从返回类型、普通值与read／write／inout参数的数组形状和名义记录实参推导；嵌套函数签名同样沿其已绑定类型交接。重复静态推导必须一致，const允许值域与最终完整签名／访问规则保持。推导只形成原const实例，不增加运行期参数、类型表或资源所有者。

普通泛型调用可从cfn签名的返回、普通值及read／write／inout参数中的类型形参推导实参，包括嵌套数组、名义记录及函数签名。推导消费已经绑定的名义类型引用，继续检查类型集合、重复T的一致性、签名参数数量和访问方式；不改变函数地址载荷或ABI。

普通值参数、read／write／inout的实际位置、连续布局与互斥借用共用原C调用ABI。cfn<R,P...>后可写requires nonzero(argument0)声明按值整数标量／向量或cfn参数及其字段的非零输入前提，也可写ensures initialized(argument0,returned>=0)声明条件输出，以及ensures memory(none／read／write／inout)声明不透明内存方向。argument0、argument1等按实际参数从零编号；字段路径选择真实输入或输出分量。调用目标本身的非零许可与各实参的输入义务分别检查；调用点沿原Proof消费已求值输入快照，后续实参改写原变量不改变先前传入的值，不插入运行时判空。条件沿原纯OutputCondition、调用前按值输入快照与本次实际返回值交接，使用原Proof／Position推导初始化；void调用只能引用真实输入，没有returned。没有条件合同的write仍承诺正常返回时完整初始化，不能把可能未写的C输出声明为无条件write。合同属于完整签名身份，条款顺序／重复规范化，模块常量绑定实际类型和值。合同内已有的argument位置及returned分别绑定当前签名的参数与实际返回值，其余泛型const引用沿源声明作用域求值。具体函数值类型作为泛型实参时携带原合同。合同差异属于签名身份，即使物理布局同为8字节。具体合同在声明类型闭合时检查全部操作数、一元／二元运算与最终bool类型，未调用的命名声明及函数值同样适用；泛型沿原实例化取得真实类型后检查。声明与执行共用原字面量转换、数值／向量／矩阵及身份比较规则，声明不生成寄存器或执行IR。dataset输入／返回的条件读取原存储长度，initialized输出仍描述实际元素及字段。声明类型闭合时已检查完整条件表达式；字面量按原运算类型、转换及常量求值规则绑定真实类型和位模式，与同类型同值模块常量及const实参形成同一签名身份。不同类型、值、运算及浮点实际精度仍区分，不作代数等价求解。f32／f64比较在原Proof条件图中保存实际运算、类型、输入快照及其真／假结果，命名／间接调用、普通函数与独立模块交接同一谓词。相等与不等互为否定；NaN使!(a<b)不等于a>=b，浮点数值相等也不产生存储位相同事实。已知浮点常量的位模式沿原capture交接实参身份，不推导浮点区间或次序；输入改写和循环回边仍沿原来源失效。头文件与实现仍在加载时匹配效果、ABI、泛型声明、参数名、访问、目标及函数自身合同；其中的函数值类型在普通具体声明或实际泛型实例闭合后，沿同一World绑定签名与数值身份再比较。只检查实际形成的实例，不将一个实例相等当成两个开放模板普遍等价；真实类型、值、运算与合同差异仍拒绝。never调用结束当前控制流，实际返回沿原寄存器／栈和大记录返回槽。循环中的改写只撤回真实签名所写位置的内容事实；函数值复制不延长代码或上下文寿命。

## 64位原生数值与资源表示
`u64`与`i64`是普通整数标量，大小与对齐均为8字节；u32、i32、f32、bool保持4字节大小与对齐。结构、数组、enum／result载荷、dataset、view及tile沿原Ty偏移交接实际位宽；64位值占两个物理u32存储字，逻辑分量数仍为一。没有增加公开的64位向量类型名，现有32位向量不改变布局。

十进制或十六进制整数字面量可显式带`u64`后缀，例如`4294967296u64`、`0xffffffffffffffffu64`；此后缀固定类型，不允许隐式降宽。无后缀整数仍按原u32／i32字面量规则处理。负的u64后缀字面量形成i64，允许`-9223372036854775808u64`；超出相应范围报B123／B180。其他有符号值可显式使用`i64(...)`，没有独立i64后缀。

整数加减乘及位操作按实际32或64位保留低位；移位量分别取低五位或低六位，i32／i64右移保留符号。除法向零截断，余数随被除数符号；MIN／−1返回同位宽MIN与零余数。动态除数沿原当前值、非零和模块合同逐分量证明，低32位为零的64位非零值不能被误判为零。数值范围与比较保留实际Scalar宽度及有符号顺序，不把64位值强制投影到u32范围。

普通记录转数值向量时，各分量按自身实际标量类型依序转换，常量与运行期结果一致；不能以首字段的类型解释其他字段。转换后的非零与范围也逐分量成立，u64高位非零不保证缩窄后的u32非零。无符号整数扩宽到更宽无符号整数保留原数值身份与算术关系，可在严格顺序分支内继续使用扩宽端点的正差。其他整数转换不沿用源值身份或旧算术域；共同范围保存实际源／目标标量类型，按真实转换后的区间与零值编码交接范围及非零性质。同宽符号转换经普通返回与独立对象／归档仍可消费该关系；缩窄可能得到零，不能直接继承源非零。无损无符号扩宽继续保留前段转换的原共同范围，不将其改写为从最初源类型直达最终类型；真正改变数值编码的连续转换只有等价于单次转换时才合并。普通值参数经过整数同宽符号转换或扩宽时，原零／非零Condition沿普通返回和独立模块继续交接；它只表达是否为零，数值范围及身份仍按实际Scalar转换。缩窄仍清空原条件，避免把非零输入转换得到的零冒认为非零。dataset整数连续转换按原输入类型、实际保留低位、符号填充终点和当前类型交接一份数值关系；每次转换直接规整该关系，普通返回和独立模块按本次实参取得目标类型的有序范围。零／非零及下标消费者共用该结果，不把i8负值经u32再扩宽当作直接扩宽，也不把截断前的非零许可沿用到结果。共同范围不代表不同元素的值相等；未知输入仍须满足真实前提，实际改写撤回旧数值事实。浮点转整数不套用该整数关系。地址位模式关系独立交接，不因此获得数值范围或索引许可。

显式整数转换按源数值符号与目标位宽处理：有符号源扩展符号，无符号源扩展零，变窄保留目标低位；i64／u64之间保留同一位模式。整数与浮点的转换及64位浮点位解释接口见下节；`bitcast_u32`／`bitcast_f32`继续只解释32位分量。

普通CPU调用、外部Windows x64 C ABI、编译对象／静态库和生成C++／Rust算法接口均按实际大小交接u64／i64。内置归约、扫描、分段及tile的数值与按位整数运算，自定义值函数集合和GPU subgroup支持相应宽标量；摘要容量按类型物理大小，寄存器参数按逻辑分量绑定。GPU数值依赖shaderInt64；64位subgroup还需shaderSubgroupExtendedTypes，原Device按实际程序查询并启用，不提供CPU subgroup替代。f32的既有精度能力要求保持。

数值宽度不授予地址、资源或索引能力。当前dataset长度／下标、const形状与枚举判别字仍为u32／i32原合同。u64／i64的dataset及CPU／GPU tile原生64位布局与原子发射已支持，GPU64位tile的当前原生缺陷见块内原子章节；窄原子与浮点原子仍未实现；实际原子、窄整数及数值排序范围见各自章节。address值及指针宽整数按原生地址章节定义；外部地址的CPU位置读写与调用借用按上节支持；自有地址的完整初始化结果与寿命交接、需执行上下文的回调及其保留寿命、通用字符串、拥有元素子资源的完整长度关系／批量增长和其他CPU后端仍未实现。源码、Release、原生、引擎采用及提交／发布状态分别见Issues。

## f64原生浮点值
`f64`是8字节大小、8字节对齐的浮点标量，一个逻辑分量占两个物理u32字。普通函数、记录、固定数组、enum／result载荷、泛型与模块、dataset／view／tile沿原Ty布局交接；floating、numeric与scalar_numeric类型族包含f64，不新增64位浮点向量名称。CPU控制位置、资源驻留及真实完成继续分别由原Frame和运行库拥有。

十进制数值带精确小写`f64`后缀固定为f64，例如`1.0000000000000002f64`、`1e100f64`、`1f64`。字面量须处于有限f64范围；无后缀小数与f后缀仍按原f32解析，再发生的显式转换不能恢复已丢失的精度。十六进制整数的末尾f64仍是合法数字，`0x1f64`为u32整数；需要宽整数时使用原u64后缀。

加减乘除、比较、取负及显式数值转换按实际浮点位宽生成；f64普通常量的这些运算可在原常量求值器中完成，f32普通常量比较同样沿原求值器采用其真实IEEE结果。普通参数及共同元素的精确浮点值可代入原比较条件，类型化来源仍属于本次实际快照；未精确范围不被猜成常量。NaN保留无序比较、正负零相等，不把!(a<b)当作a>=b。固定f64字面量不隐式降为f32，两个已确定的不同数值类型要求显式转换。浮点转整数沿原有效输入合同向零截断，调用者须提供目标有效范围内的有限数；不对NaN、无穷和越界值钳制或返回假成功。`bitcast_u64(f64值)`与`bitcast_f64(u64值)`只解释完整64位标量位模式，不是数值转换，不授予地址或句柄权限。输出使用17位有效数字，原f32保持9位。

Windows x64普通调用和外部C声明按真实double／Rust f64签名交接。只有浮点标量参数和返回值使用相应XMM浮点ABI；含一个f64字段的普通记录仍按该记录的C ABI分类。机器装载、存储、溢出槽与位解释均消费原Scalar宽度；resize和普通值sequence继续消费完整Ty分量。

GPU实际使用f64类型需要shaderFloat64；接口中的未解释载荷仅作整数存储复制、未参与实际浮点类型使用时，不因其f64声明额外要求该能力。阶段寄存器、局部、自定义值函数及内建归约／扫描的真实f64类型仍须声明；64位subgroup还需shaderSubgroupExtendedTypes。实际浮点运算、不同浮点宽度的转换及浮点转bool的比较按涉及位宽声明DenormPreserve与SignedZeroInfNanPreserve；整数与浮点之间的转换、bool转浮点及同类型值交接本身不声明这两种模式，实际类型能力仍须具备。程序整体需要的两种浮点位宽保留模式由原DeviceRequirements.float32_preserve／float64_preserve交接，来源是各原Stage的实际运算需求；ProgramLayout保存并随执行映像格式执行映像编码，GPU装入消费该合同。Shader各入口仍按实际操作发射模式，运行阶段不另保存同一程序汇总。设备查询、启用和装入在原Vulkan所有者内完成。显式选定设备不具备所需能力时明确拒绝；默认入口在首次创建逻辑设备前，按原DeviceRequirements特性、计算队列及必要设备扩展匹配真实物理设备，并以实际uniform_bytes及原纹理数组元素总数匹配候选的推送常量与纹理描述符容量，并从实际GPU映像的ExecutionPlan取得工作组宽度及发射后的共享内存峰值参与同一选择。同一程序的普通／辅助storage binding与纹理数组元素的组合总数也参与候选maxPerStageResources匹配。数值语义不因缺失能力改变，GPU工作不转交CPU；创建后的设备身份保持，创建／装入／执行失败直接返回。SDK可显式选择真实物理设备，C算法绑定的GPU入口复用同一默认创建路径；完整资源限额选卡仍见Issues C-022.5.4.3。

CPU数学直接使用平台C ABI的f64函数。GPU的浮点取负与abs在B发射器中共用同宽无符号位转换，分别翻转与清除符号位，保留其余位。floor、sqrt和min／max由B发射GLSL.std.450指令，既有64位数值集合／subgroup运算沿各自原入口；Sin、Cos、Pow、Exp、Exp2、Log、Log2的GLSL.std.450指令不接受64位浮点，尚未实现的GPU f64超越函数明确报错。该限制不构成永久语言边界，也不表示全部输入上的严格浮点精度策略、浮点原子或所有64位平台已完成。GPU64位tile的实现与当前原生缺陷见块内原子章节。

BLJson::Number所需的f64只是基础数值能力；字符串、递归容器及JSON职责的实际迁移分别按真实缺口记录。

## 词法与名称身份

B 与 C 一致，关键字和标识符区分大小写。`value`、`Value`、`VALUE` 是不同标识符，可以在相应作用域规则允许时分别命名；关键字必须使用规范中的精确拼写，例如 `module` 不等于 `Module`。模块、类型、字段、函数、参数和局部名称均按原始拼写确定身份。

`phase` 按完整阶段语句作为上下文语法词识别：语句位置的 `phase;` 表达原全域阶段边界；在声明名称及普通值表达式中，`phase` 仍按原作用域、名称和类型规则使用。`phase = value;`、`phase += value;`、`phase(value);` 及该名称的字段／数组位置不会因首个词相同被解析为阶段语句。这不改变其他语法词的文法或名称规则，`Phase` 与 `PHASE` 也不成为阶段语法。

这是确定的语言规格边界，沿用当前实现。词法、名称解析、模块接口以及生成的 C／C++／Rust 互操作接口必须保留合法名称的区别；因目标语言约束进行名称编码时也不能合并这些身份。除非用户明确要求变更语言规格，不能在重构、优化、修复或跨平台适配中改为忽略大小写，或增加大小写归一化别名与兼容路径。

## 文本字面量

普通双引号文本是既有固定字节数组 `array<u8,N>`，`N` 为解码后实际 UTF-8 字节数。`"ABC"` 有三个字节，`"中文🌍"` 有十个字节，`""` 是零长数组；不自动追加 NUL。可直接写 Unicode 与实际换行，转义支持 `\\`、`\"`、`\n`、`\r`、`\t`、`\0` 和 `\u{十六进制Unicode标量}`。例如 `"A\u{4E2D}\0"` 包含五个字节，最后一个为显式零字节。非法转义、代理码点和超出 Unicode 范围的值报 B385，未闭合引号报 B003，不替换或截断字符。

字面量直接使用原数组的常量、泛型、记录、按值复制／返回、访问与借用规则，数量和布局沿原数组规则；不会隐式成为地址、view 或堆文本。数组后续允许的逐字节修改不保证仍是合法 UTF-8。C 字符串所需终止符由调用者明确提供，Windows UTF-16 等编码沿标准 Unicode 库显式转换，地址寿命沿实际数组／dataset 及其消费者保持。动态文本容器和完整字符串库仍是独立缺口。

## 统一函数与实际执行位置
`program` 已从源语言移除，独立的 `program` 单词不能再用作声明关键字或标识符；词法阶段在其真实文件、行列报 B004，并说明应改用返回类型函数定义（无返回值用 `void`），并行域由 `parallel` 表达。该错误不依赖函数是否被调用，也不允许通过声明同名类型使旧写法变成合法函数。导入的 `.h`／`.b` 使用同一词法规则。注释和字符串不受此限制，`Program`、`PROGRAM` 和含有该子串的其他名称继续按区分大小写的名称规则处理。内部 `Program` 及 `--program` 产物入口选项不是源声明语法。

普通值计算与整域计算均使用返回类型、函数名和参数定义；不再声明源语言 `program` 类别。`void step(inout dataset<u32> rows) on compute` 可在函数体内写 `parallel(v in rows)`，组合函数直接调用 `step(rows)`。在 CPU 主流程中，普通调用沿当前 CPU 位置执行，也可直接使用 parallel；参与范围由定义及实际调用关系推导，不要求手写 launch 许可。直接parallel按本次实际捕获取得输入合同；同一语法位置的初始轮与后续静态展开分别使用各自的当前字段事实，不要求后续值继续满足首轮初值。重复执行的通用回边按parallel的实际资源和字段写撤回被覆盖的旧共同数值；直接赋值及普通位置调用使用同一字段来源，未指定某个下标的未来写也按同一资源的实际字节区分字段；未写兄弟字段继续保留已有前提。具名整域函数继续按其正式输入合同共享实现。初始化、借用、驻留和每次完成仍按原资源规则处理。

`on cpu`／`on gpu`／`on compute` 声明函数可用目标，不选择本次执行设备。`target(cpu, step(rows))`／`target(gpu, step(rows))` 选择本次位置，只包围一次函数调用。CPU target 同样支持普通值、拥有值和 try 的既有返回交接；GPU target 当前接收以借用 dataset 与普通统一参数组成的整域 void 函数。普通值跨 GPU 返回、元素内嵌套域、一般异步应用语法尚未实现，明确报错，不隐式换目标。

同步 GPU target 通过原真实提交的 wait 完成本次执行后返回，结果保留在 GPU；download 负责向 CPU 交接。不会隐式下载，不等待无关的全设备工作。原低层异步 SDK 仍持有其真实完成对象及资源。

当前参与要求在装入的定义／调用闭包中推导，纯域组合沿实际调用及被调闭合参与合同保持Domain，与实现来自源码、对象或归档无关；on cpu本身及被调方已编译都不产生Host要求。真正的分配、普通CPU局部求值／控制及Host调用仍沿原推导保持Host。已编译子域由原DomainCall处理，保持原目标及前缀／资源合同，不要求重读B正文。已编译整域模块导出当前支持的参与要求和初始化效果，调用者消费产物接口。CPU应用普通原生调用／递归及有限独立模块按后文规则实现；完整效果／资源阶段、其他目标普通调用、选择性内联与完整泛型分发仍待实现。Program／Stage／ExecutionPlan、compile_entry和--program保留实际算法产物职责。

普通条件、循环和调用保持各自语义；当前parallel及target不表示任意顺序程序会自动变为通用任务DAG。编译器内部依赖／事实图与现有执行计划各有职责。域内分片、参与入口、远端数据引用及完整明确机器方案仍为待实现设计，不构成新的现有语法。

## 只有接口的模块
`.h` 是接口事实源；仅含数据类型、类型集合、常量或外部声明的模块不需要空 `.b`。实现文件实际存在时才参与加载，私有声明仍只在本模块可见。普通函数声明必须有真实匹配函数体；缺失实现报 B027，接口内普通函数体报 B026，模块名不一致报 B022，实际实现文件不可读取报 B020。不会以空实现对象代替文件，也不忽略真实 I/O 错误。

普通调用先按当前模块、直接导入和接口可见性解析callee；未知或未导出的函数在调用处报B158，不把其已声明的dataset等资源实参误报为未知普通值。可见函数与已确定的内建能力继续按各自签名检查实参，实参按源码顺序各求值一次。

模块导入关系可以形成合法函数闭包，不等同于按值类型递归；数据布局和常量求值仍在各自所有者处检查，分别保留B032／B160。接口声明仍必须有真实匹配实现。返回及参数按实际类型身份匹配：u32／uint、i32／int、f32／float、u64／usize、i64／isize，以及解析到同一声明的非限定／完整限定名义引用可以使用不同合法拼写；嵌套类型、已代入的类型／const实参与函数值合同沿同一原类型引用语义交接，不提前展开完整递归记录布局。texture2d／texture3d及acceleration保留原资源引用身份；纹理维度、实际声明的格式与固定资源数组形状参与匹配，格式标记不是普通类型。资源的访问、目标与寿命继续由原资源合同判定。真正不同的类型、形状、访问、效果、目标、参数名和大小写仍各按原规则核对，不能用相同布局或已遇到模块名代替实现检查。

独立模块的`bc -c file.b --module -o file.obj`与普通应用可在同一次编译中组合源码依赖和`--import`对象／归档。入口使用本次源；每个非根模块若有实际产物提供者，只使用其中的接口、已导出具体实例和函数事实，不读同名磁盘.h／.b；其余依赖沿-I根捕获真实.h及可选.b，包含实现中的私有导入和纯类型／常量模块。 可选.b由实际打开结果确认是否存在。模块装入遇到必需.h缺失、实际访问或读取错误、无效UTF-8时报告B020。模块来源在本次输入捕获与装入中一致固定；提供者损坏、合同不符或缺少具体实例时直接报真实错误，不改读其实现。源码普通函数确实缺少.b／匹配函数体时仍准确报错，真实读取和解析错误不忽略。当前模块只交付自己拥有的代码与实际符号依赖，其他模块的私有实现不成为额外导入。函数合同导出与对象读取交接同一规范编码；末尾空借用槽按原缺省读取语义省略，中间位置和非空来源及其条件保留。链接仍精确比较类型、布局、访问和函数事实；内部空槽数量不构成接口变化，既有对象读取不会修改原文件。随后仍由独立blink消费目标文件，不在链接时重读B源或生成代码。

资源位置合同沿原Data拥有树交接根、元素及其拥有子资源，拥有记录、固定数组、活动变体和普通read／inout整份借用消费同一循环生命周期比较。原Same位置义务一侧已确定CPU／GPU时，沿同一条件分支查询导出另一侧实际输入的位置要求，最终比较继续消费这些要求。元素序数在规范化、本地消费和调用采用中沿原输入深度绑定；数值／借用事实失效保持原资源位置，实际拥有赋值交接新位置。同一完整访问载荷的使用条件在原Interface访问合流处取析取；来源、目标、布局／索引、权限、原子性、初始化及写回值有任何差异的要求仍分别交接。单条或完全相同的条件沿用原Condition。来源、循环输出和初始化先由原所有者闭合；访问收敛沿当前未闭合函数的原Function.calls判断本函数访问输出是否会回到自身，直接及相互递归中同载荷的不同条件合流才消费原真假两根规整。没有自身反馈的单向调用链保留完整共享条件图，随实际callee输入稳定，不为内部比较预先展开路径项。可回达查询的临时访问集合及选择结果只属于本次原求解调用，不复制调用图、不增加调度器或持久缓存。每轮临时合流结果直接交回原访问集合，不保留按使用点或递归展开次数区分的重复要求；实际条件查询继续消费完整图，公开合同继续在原编码出口规范真实别名及条件，不保存使用点数量。检查期Clause沿原事实规则排序、合取去重、吸收与共识闭合；公开出口则由同一Contract所有者规范返回、驻留与访问条件的临时副本。原ABI给出实际Scalar及dataset根／长度，无符号下界一／非零及上界零／零沿原Fact共用既有别名，其他数值界不跨Scalar改写；整数零／非零、上下界、比较和已知长度索引使用同一有序区间语义，真假两根分别按固定值／原子顺序规约，再沿原Proof／Clause共享DAG编码。相邻同结果区间合并；不展开全部析取路径，不逐位拆解数值比较。已有标量上下界按其Scalar既定的有序位域进入同一有限区间；浮点Minimum／Maximum也保留该次序，下界的相反条件与同义上界只表示同一关系。f32／f64的原位映射保留正负零及NaN位模式；浮点比较谓词继续原无序语义，不改写为整数比较或另一比较的补集。数值尾部、借用、布局、初始化、存储版本及浮点谓词保持，未识别关系仍是原类型化原子。完整目标、索引、权限、原子性、来源与写回值均相同的要求才合流条件；实际B读取、写入、顺序和完成不变。临时决策材料随编码结束释放，原检查事实不被改写。模块导出与既有COFF读取共用原Contract编解码。原生对象保留实际机器符号、节重定位及展开信息。只有公开入口、显式请求的实例及这些入口经原返回、写回与位置调用关系实际引用的私有定义交付语义合同。本模块任务的工作签名仍由同一实际函数体提供，独立模块的任务调用读取公开工作函数的导入合同。链接继续严格比较实际导入合同。有限布尔结构规约不承诺一般算术／程序等价、完整bool事实推导或模块版本迁移，完整泛型分发、资源阶段及版本演化继续按实际缺口推进。

## 接口、实现与并行域

首个消费者 BLEngine/Programs 中的实际计算接口（算法属于引擎，语言与框架独立构建）：

```c
module simulation.bodies;
import math.field;

struct BLBody<T: floating, const Power: u32 in {1,2,3}> {
    T position;
    T velocity;
    f32 energy;
};
typeset BLSimulatedBody {
    BLBody<f32,1>; BLBody<f32,2>; BLBody<f32,3>;
    BLBody<float2,1>; BLBody<float2,2>; BLBody<float2,3>;
    BLBody<float3,1>; BLBody<float3,2>; BLBody<float3,3>;
    BLBody<float4,1>; BLBody<float4,2>; BLBody<float4,3>;
};
void integrate_body<T: floating, const Power: u32 in {1,2,3}>(inout BLBody<T,Power> body, f32 dt, f32 strength) on compute;
void integrate<B: BLSimulatedBody>(inout dataset<B> bodies, f32 dt, f32 strength) on compute;
```

对应 `.b`：

```c
module simulation.bodies;

void integrate_body<T: floating, const Power: u32 in {1,2,3}>(inout BLBody<T,Power> body, f32 dt, f32 strength) on compute {
    body.velocity += central_acceleration<Power>(body.position, strength) * dt;
    body.position += body.velocity * dt;
}
void integrate<B: BLSimulatedBody>(inout dataset<B> bodies, f32 dt, f32 strength) on compute {
    parallel (body in bodies) { integrate_body(body, dt, strength); }
}
```

这段源码直接生成 CPU 范围函数与 GPU 计算程序。`body` 是当前域的一个独占元素视图；它不表示可以随意访问其他线程正在修改的数据。`on compute` 表示两个后端都可执行，`on cpu`／`on gpu` 则限制设备；不能自动切换设备来掩盖执行失败。

`计算函数` 可以组合其他 计算函数，编译器展开阶段依赖。只接收普通值的函数支持结构体、向量和不可变 view 参数；函数的资源访问由 read dataset、atomic dataset、atomic 字段及 read／write／inout 普通位置借用显式声明；CPU 应用还可将整份已初始化 dataset 以 inout 借给普通 CPU 函数。CPU应用主流程的普通调用使用后述原生函数与调用帧；CPU parallel范围中明确on cpu的普通源函数沿原生函数调用；Compute共享函数保留原内联选择，CPU递归回边交给同一普通函数编译器和代码所有者。通用目标的预检保留递归调用，实际GPU递归仍拒绝；GPU／cooperative的其余调用继续按实际目标和参与范围专门化、内联，完整设备普通调用与选择性内联继续研发。普通函数可以在条件与顺序循环中提前 return；有返回值函数的所有可继续到函数末尾的路径都必须返回匹配类型，void 函数可自然结束或以 return; 结束。return 只退出当前函数，不退出 parallel 实例。cooperative 函数暂时仍保持末尾返回。

## 模块类型名称与泛型绑定
`.h` 声明的记录、枚举及 typeset 对直接导入该模块的源码公开，`.b` 声明的对应名称只在所属模块可见。普通值、记录、枚举及类型集合共用原模块可见性规则：引用其他模块的完整限定名也需要直接导入；经其他模块间接装入了声明，不会使其名称自动进入当前模块。解析同名时，当前模块自己的声明优先，不可见的私有声明和重复导入同一声明均不构成歧义。

普通调用先使用原局部函数值、本模块及唯一可见普通声明和实际泛型实例，再处理确实未绑定名称的内建操作；resize、take_last、own、into_inner不是禁止声明的保留名。可见名称查询区分无候选、唯一完整名称及多个不同候选；多个可见普通声明不按参数、返回类型、导入顺序或同名内建选择目标，实际调用须使用明确的模块限定名。类型、访问效果和循环长度预分析消费同一原查询事实，不把歧义当成未绑定，也不在缺少局部绑定上下文的查询处提前拒绝合法遮蔽。真正资源操作的初始化、移动与最后释放仍由原资源所有者处理。

泛型调用先在调用者处确定类型实参，之后沿实际名义类型实例化。通用库可以接收并返回调用方的私有记录／枚举，组合其数组、记录和结果；通用库无需反向导入调用者。通过已导入的公开接口取得的值也可以继续传给泛型函数，不必重新写出其定义模块的类型名称。显式写出的类型实参仍须在调用者作用域可见，不能借泛型调用构造其他模块的私有类型。

名义类型身份沿调用绑定交接。dataset字段保存已绑定模块、类型及const的元素引用，描述值本身具有有限布局；例如记录可通过dataset<Node>拥有同类型的子记录，互递归记录也沿各自实际元素类型解析。直接按值内联自身仍没有有限布局，不能因隔着类型引用而通过检查。普通字段、变体和固定数组继续使用真实值布局；函数签名和泛型实参不因出现在类型中而成为存储字段。类型引用不增加运行时类型对象、查找或分派。字段访问控制、公开接口依赖完整性及完整模块封装仍待实现。

## 类型值表示的编译期大小与对齐

size_of<T>()和align_of<T>()分别返回具体类型已有值表示的字节大小与对齐，结果是usize（u64）编译期常量。必须显式提供一个类型且不接收值实参；可以用于普通表达式、模块const和具体泛型实例。const形状仍按原u32规则显式转换，不新增另一种形状类型。名称按现有函数／类型解析，已绑定的同名声明继续使用原语义。

类型沿原expected_storage／World.ty解析，结果直接来自同一Ty.size／Ty.align；不构造或求值T对象、不执行释放函数，也不生成运行时反射、布局表或新的执行节点。普通标量、向量／矩阵、记录、固定数组、enum／result、address／handle／cfn及合法泛型／const实例均保持其现有布局。dataset查询的是值描述表示（当前8字节、对齐4），view查询的是引用值表示（当前24字节、对齐8），owned<T,F>保持T载荷布局；不是实际数据分配量、指向内容大小或Rust Frame私有布局。查询不授予资源复制、外部访问或C ABI交接权限。void、never、tile及未知／未闭合类型继续由原类型规则诊断。

大小、对齐及数组步长分别使用原布局语义：size_of<float3>()为12、align_of<float3>()为16，size_of<array<float3,2>>()为32；不能把值字节范围自动圆整成另一份布局。记录stride及字段偏移仍由原Ty决定。已编译函数体交接常量和机器代码；公开接口中的查询表达式由支持该语法的消费者解析，原对象／资源编码版本不因本查询改变。

## 类型与字段布局的结构查询

bc的文件／目录参数保持操作系统原生路径编码，包括源码、模块根、导入、产物、观察材料与输出路径；不会仅因Windows路径含非Unicode代码单元而在命令分派前中止。选项、类型／入口／实例／符号及数值仍按各自文本语义解析。layout／ir／x64命令上下文及image输入／输出的JSON路径字段保持可表示的字符串，否则为null；对应的`<字段>_encoded_bytes`为该路径原生OsStr编码的十六进制字节，IR的`imports_encoded_bytes`与`imports`按同一次序对应。`bc inputs`保持原有`text`／`encoded_bytes`结构；原始字节与显示文本不互相冒充，不使用替换字符恢复路径。算法库对象文件名同时作为导出符号前缀时仍须满足既有文本ABI要求。

`bc layout <module.h|module.b> <type> [-I module-root] [--field path]`读取本次明确源码模块，沿原Parser、World和Ty取得具体值类型，以单个JSON对象输出。它不生成CPU／GPU代码、不创建运行资源，也不读取目标程序的运行值；源码／接口装入、可见性、泛型／const、布局和合同仍使用原规则。纯类型接口不需要空.b；有真实函数的模块继续遵守原实现装入规则。查询成功只说明本次类型描述可取得，不保证该类型在所有存储、调用或目标上均已能执行。

```powershell
bc layout src/types.h 'Row' -I src
bc layout src/types.h 'array<Row,2>' -I src --field '[1].payload.c'
```

`--field`复用原Ty::field，支持嵌套字段、固定数组下标和显式变体载荷路径，返回相对根值的字节偏移；不存在或越界的路径直接报错。变体路径描述静态重叠布局，不表示该分支正在活动。查询不跨view／dataset解引用动态地址，也不求值用户函数。外层kind为type_layout，phase为checked_type，source_path／module_root是本次绝对来源，request、root_type、field及offset_bytes说明所选范围；源码查询不冒充旧产物的布局恢复。

layout来自原Ty：name／kind、size_bytes、align_bytes、stride_bytes、类型／const实参、components、fields、variants、array、view、dataset与cleanup。字段偏移相对该层记录，变体输出实际discriminant和payload_offset_bytes，数组输出count和element_stride_bytes。view／dataset的值描述布局与元素存储布局分开，dataset固定数量未确定时为null，其element_reference保存已绑定的元素名称。源码查询在本次World仍存活时解析该引用，输出element_stride_bytes及element；递归中已经展开的同一类型以kind=type_reference和name交接，不无限复制类型树。类型本身不提供实际分配容量、地址、活动变体或资源寿命。

components保留Ty的实际Scalar分量、字节偏移、宽度和覆盖的四字节资源字。resource_word_count是该值从相对字节零起的覆盖字数；first_resource_word／byte_in_word等同样相对当前值，不能把窄字段覆盖的同一字重复计为多份分配。嵌套位置需结合父偏移，实际地址和载荷长度仍由原运行所有者提供。带载荷变体的顶层components可包含判别字和载荷的原存储分量，具体声明类型在各variants中；四字节资源字与逻辑标量、CPU寄存器、SIMD lane和GPU物理寄存器分别理解。float3的size_bytes为12、align_bytes为16、stride_bytes为16，不能把对齐填充当成值字段。

Rust API `Compiler::type_layout(path, root, request)`返回原Ty；`Compiler::type_layout_json(path, root, request, field)`在本次源码World内取得根类型名、所选字节偏移和布局JSON，CLI使用同一入口解析dataset元素引用。`Ty::layout_json()`只序列化已有值布局，dataset保留element_reference，不重开源码解析引用。`blanguage::ir::interface_layout_json(&program)`直接消费已有ProgramInterface，输出kind=interface_layout、原程序目标、uniform_bytes、buffer实际binding／access／stride／固定数量／已有元素布局，以及uniform原offset和类型；其中dataset引用也只交接原名称。接口查询可用于已编译执行映像，不重读源码、不调用Compiler，运行角色无需启用compiler功能。两类查询各消费原所有者持有的材料，不增加运行时反射或另一布局权威，也不改变运行布局。

`bc build`沿原完整生成路径在既有.layout之外导出.layout.json，其中是本次程序的buffer／uniform接口布局；该JSON不声称包含全部阶段、共享存储、栈或运行状态。已有.x64／.reloc／.spv／.layout继续按原职责输出；新增文件属于调用者指定的生成目录，正常应用执行不因此序列化布局或保留另一份状态。总体工具设计与剩余能力见Architecture对应职责及双台账。

## 实际IR的按需结构查询

`bc ir <module.b> [-I root] [--module | --program name | --entry name] [--function qualified-name | --stage index] [--import object|archive] [--instance name]`通过原Compiler编译本次明确源码请求，随后只读查询同一返回结果。默认--module；--program选择具体并行入口，--entry选择普通CPU应用入口。三种模式互斥，入口的泛型／const实参直接写在名字中；模块开放实例用--instance，可重复请求不同实例。--import沿原已编译模块／归档装入，只用于模块或应用；直接并行入口的原API当前不接受已编译导入，可在模块查询中查看实际资源程序。

```powershell
bc ir sample/lessons/map.b -I sample --program demo --stage 0
bc ir sample/hybrid/Programs/simulation/bodies.b -I sample/hybrid/Programs --program 'integrate<BLBody<float3,1>>'
bc ir src/math.b -I src --module --instance 'mix<7>' --function 'math::mix<7>'
bc ir src/app.b -I src --entry main --import target/math.obj
```

上述map并行入口的阶段查询可直接取得indexed_store_read_forward材料：retained为seed中保留的值生产者，reads各项分别交接实际Store与scale被替代读取的来源。查询本次实际编译模式；普通模块／CPU应用的不同IR路径不能冒称已经观测到该并行入口的替换。来源字段及未完成输入范围见后节“索引Store写入值到保留指令的读取来源”。

物理模拟integrate入口可从原Load的generated_origin查询indexed_read_reuse：retained保留position首次读取，reused给出更新位置时被替代读取。当前Load的word与所属Scalar分别解释实际分量；这些静态位置不是某次运行地址。该来源与map入口的indexed_store_read_forward分别表示不同生产者。

--function使用完整、区分大小写的实际函数实例名，只从本次模块／应用保留的functions选择；缺失函数直接失败，不重编另一函数或推测已内联函数体。--stage是本次--program的零起优化后阶段编号，越界直接失败；优化合并后的阶段数可以少于source_stages。未选函数／阶段时返回该编译结果的完整当前IR；未实例化的开放泛型没有伪造函数体。

结果是单个JSON：kind=ir_query，source_path／module_root及其原生编码字段交接本次绝对路径，文本不可表示时原字段为null；mode、entry、function、stage、imports和instances说明实际请求。phase分别是optimized_program_ir、finalized_application_ir或finalized_module_ir，表示原入口已经完成的优化／函数及资源整理；不表示已独立提供Debug／Release或优化级别选择。ir来自原返回对象，源码查询不输出COFF、EXE或入口映像、不创建运行资源；source_path不是完整构建输入身份。已有产物的原请求／来源继续用bc inputs；没有输入记录或IR的对象不会被查询器重新解释成历史事实。

Program::ir_json输出program_ir：原interface布局、source_origin／generated_origin、source_stages、accelerations／textures、prefix及长度条件和实际stages。Stage::ir_json输出stage_ir：domain、prefix／selection、operation、boundary_before、registers、locals、code、reads／writes／cross_access及tiles。ValueCode::ir_json输出value_ir，保留inputs／outputs和同一寄存器／局部值／指令；FunctionBody::ir_json输出function_ir，保留完整名称、参数值／引用类型及访问、返回类型和body。Application／Module的ir_json分别输出application_ir／module_ir，包括实际保留函数、导入名称及原资源类型／程序；应用另有原入口body，模块另有原exports。已编译导入可能只有合同和调用目标，没有提供者IR；缺失函数体不会补造。

每条指令用kind及原字段表达，If、Loop和Scope保留真实嵌套；调用保留明确的defined／external／indirect目标、值／引用实参、位置和memory关系，原子保留实际order及比较交换failure。自定义归约／扫描的project、identity、combine继续是原ValueCode，不另外展开或发射内核。Constant的bits及整数常数除数使用固定16位十六进制字符串，完整保留u64、NaN载荷、无穷和负零的原位；其解释由对应Scalar决定，不通过JSON浮点数丢失精度。

registers数组下标是当前Stage／ValueCode的逻辑寄存器，Scalar说明逻辑位宽；locals的end_component是原声明结束的逻辑分量位置，填充不占分量。字段名及word、slot、stride、binding等关系保留原IR语义，不能当实际机器寄存器或远端地址；大小／对齐／字节偏移以嵌入的原Ty布局及接口为准。指令数组位置只说明当前容器顺序，不是跨编译稳定身份、执行轨迹或机器指令区间。length_requirements中的same_length以left／right保存实际两端的等长要求，没有数据复制方向。

SDK可直接对原编译返回的Program、Application、Module调用ir_json，对应用／模块用function_ir_json(name)选择实际保留函数；这些原编译结果的查询属于compiler功能。Stage、ValueCode和FunctionBody的只读值查询不依赖compiler；它们也可由调用者构造，查询本身不验证其合法性、不证明该值已经优化或执行。所有查询只借用原值并返回独立字符串，不修改IR、保存额外快照或扩展ProgramInterface。正常执行映像的ExecutionPlan并不保留已释放的Stage／ValueCode，查询不能恢复不存在的IR，也不改变代码、优化前提、资源寿命或正常运行采集策略。

### 函数与程序IR的原定义来源

compiler功能的`ir::SourceOrigin`保存原Definition所交付的实际Source快照、函数Span、模块和原函数名。`FunctionBody.source_origin`为可选值，`Program::source_origin()`返回可选只读引用；SourceOrigin的source／span／module／definition getter借用原事实，ir_json返回独立JSON。现有FunctionBody／Program的ir_json和bc ir的普通模块、应用、具体并行入口及函数选择沿source_origin字段交接同一值，不重编已取得的IR或重新读取源文件。

值的kind为source_definition，module和definition分别保持原模块、原函数名，大小写不折叠；具体泛型类型／静态值实例名继续位于所属function_ir.name或program_ir.interface.program。不同实例可以共享同一原定义，原函数／程序范围不等于每条IR、内联调用或机器指令范围。原Definition按实际生产者区分Source、CompiledInterface和Generated；仅真实源码定义且持有原实现体时形成SourceOrigin。应用parallel提取函数与类型整数式的合成上下文、已编译接口及没有原实现的外部边界为null，不凭body存在、ABI用compiled字段为空或内部名字猜成源码函数；接口常量和手造Return／Function同样不能伪造原定义。未返回的const求值或导入合同不伪造函数体和实现来源。源定义和当前已有IR分别说明，不将原声明头当作外部实现。合成上下文中的实际实例及后续真实内联目标分别保留；缺少调用者定义时原inline_site保留已知实例和字节范围，无法解释的行列为null，不丢弃后续真实定义。空source_origin不表示没有原语法；parallel提取可沿不可变SourceSite锚点交接原语句定义与实际实例，生成函数自身仍不成为原源码函数。域展开沿下述domain_path交接，阶段索引／空前缀生成原因沿下述generated_origin交接；其余Frame／域包装生成职责继续见Issues C-021.7.9.7.2.4.5／.6。

source_path保留原Source.path的可表示UTF-8字符串，不可表示时为null；Windows的path_utf16保留完整原UTF-16单元，Unix路径以path_bytes保留原字节，不作有损替换。source_sha256是原装入UTF-8文本全部字节的SHA256，source_bytes为实际长度；摘要仅标识本次源快照，不证明当前磁盘内容、机器映像或执行授权。start_byte／end_byte为原函数声明／定义的UTF-8字节半开区间，span_unit=utf8_byte、span_range=half_open；start／end各含line和column，position_base=1，column_unit=unicode_scalar。行按LF划分，CRLF计一次换行，end是排他末端的位置，不是最后一个字符的位置。

文本仍共享原Source的不可变Arc，查询只借用，独立JSON归调用者；原IR最后消费者结束后释放，不保留World／AST、复制全文或增加来源注册表。普通编译不为观察重复计算摘要，只有显式查询承担散列和编码成本；磁盘源已改写或移开时，已取得IR仍查询原快照。运行角色关闭compiler后没有SourceOrigin字段及其输出；compiler角色手工构造FunctionBody时显式提供source_origin，合成IR可用None。B语法、C ABI、必要执行编码及数据／CPU／GPU资源寿命不因来源观察变化。

```text
bc ir sample/lessons/tiles.b -I sample --program demo<96>
bc ir sample/applications/start.b -I sample --entry main
```

当前只交付根定义到IR。逐操作和实际内联调用链、优化后的合并／消除、多对多机器字节区间、COFF及保存映像中的来源材料分别沿C-021.7.9.7剩余叶项；GPU复用共同源／IR事实后，仍须沿.8完成SPIR-V来源，不把本字段当作源级调试或最终GPU ISA映射已完成。

## 明确x64指令字节范围的结构查询

`bc x64 <instruction-bytes> [--ip file-base-ip] [--offset bytes] [--length bytes]`读取调用者明确声明为指令的原字节范围，返回单个JSON。它直接解析原字节，不装入B模块、不创建Compiler／编译工作，不生成或执行机器码。文件不是COFF／PE容器接口；Object符号／重定位、原映像主函数／辅助代码／常量范围和最终链接查询仍按各自架构／台账交付，不能把整份容器或带常量的映像直接当成全部指令。

```powershell
bc x64 target/example.x64 --ip 0x100000000 --offset 0 --length 32
```

数值参数接受十进制或0x前缀十六进制。file-base-ip是文件字节零的假定IP，默认0；offset默认0，length默认选到文件末端。所选范围的首IP等于file-base-ip加offset，按64位IP语义回绕；instruction的offset_bytes始终相对所选范围。重复、缺失、不可解析参数以及越过文件的范围直接报错，空范围明确返回零条指令。调用者须确定指令起点与完整终点；解码成功不证明输入属于代码或所有指令均可在当前CPU执行。

结果kind=x64_query，交接source_path、file_base_ip、offset_bytes、length_bytes和decode。decode的kind=x64_instructions，bitness为64，origin_ip、byte_length、instruction_count及instructions说明实际所选范围；仅使用64位CPU解码，不提供32位CPU目标。64位模式中的窄操作数、地址大小前缀与向量指令继续按实际编码解释。

每条指令保留offset_bytes、ip、next_ip、length_bytes、原bytes十六进制、code／mnemonic、encoding、flow_control、operands及operand_access。编码常量的位移／第一立即数／第二立即数分别给出相对该条指令的字节位置、长度与原字节；不存在时为null。64位地址与立即数／位移用十六进制字符串交接，不经JSON浮点数。立即数value_bits保持解码器给出的逻辑位模式，符号扩展由具体OpKind说明，encoded_constants仍保存原窄编码。

寄存器交接原名称与实际字节宽度；普通内存操作数保留段、基址、索引、scale、内存类型与大小。位移明确区分displacement的原逻辑位模式与ip_relative_target的已按假定IP解析目标，不能把RIP／EIP相对目标再次当位移相加。隐式字符串内存保留原SI／ESI／RSI或DI／EDI／RDI和真实段关系。内存大小未知时size_bytes为null，不冒充实测零字节；向量op_mask、zeroing、broadcast、rounding与异常抑制按原编码交接。

used_registers／used_memory、访问类别和flags仅是ISA描述的静态使用／影响，不读取寄存器、解引用内存或报告执行次数；REP条件访问与真实运行次数分开。flags使用OF、CF等具名标志，既不输出解码库私有掩码冒充硬件RFLAGS，也不声称已取得当前标志值。内存表达式、分支目标、异常类指令或CPUID相关语义不证明运行权限、目标数据有效、实际分支经过或硬件支持。

SDK `blanguage::native::instructions_json(bytes, ip) -> Result<String, DecodeError>`借用原字节并返回独立字符串；DecodeError含实际失败offset、ip与reason。采用正常有效性检查，非法、截断或当前不支持的编码在实际位置失败，CLI不输出部分成功JSON、不跳字节或以db填充。依赖只在compiler功能启用iced-x86的std／decoder／instr_info，默认encoder、assembler和格式器关闭；公开接口不暴露依赖类型，不改变B代码生成、资源ABI或无compiler的运行库。解码能力随工具实现演进，不成为语言可用指令的永久边界。原代码／数据范围归实际发射和产物所有者，当前独立交付状态见双台账与Architecture的tool-5系列职责。

## 生成期原生映像的范围与指令查询

compiler功能中的`NativeCode::code_json(ip)`只读取同一原发射结果，返回kind=native_code、实际byte_length、function_end、stack_bytes、原calls／relocations／unwind及regions。ip是调用者声明的字节零地址，以完整64位十六进制交接；未装入的编译结果通常使用0，不将它当实际运行地址。重定位分别交接print标量、外部library／symbol或具名function，原占位字节不是最终绑定地址；call_sites的null保持原间接调用事实。

主函数范围直接使用原function_end；compiler角色的tail_regions按原写入顺序保存每个尾部分区的排他end与kind，不另存相同主末端。输出region的kind为main_function、stack_probe、alignment或literal，offset_bytes／length_bytes相对同一bytes；各区间的原bytes十六进制合起来完整重建本次映像。main_function／stack_probe调用原instructions_json，instructions中的偏移相对本region，IP则使用ip加region偏移；字面量和对齐的instructions为null，只保留真实字节。解码失败在实际映像偏移与IP上报错，不跳过或把数据猜成指令。查询借用真实匹配的编译结果，不为调用者手工伪造的NativeCode证明合法性、实际执行或目标支持。

`StageCode::code_json()`按Element主入口、四种单入口延续、Scan的prefix／apply及Sort的全部initialize／histogram／scatter／gather返回原容器关系；`ProgramImage::code_json()`读取同一ExecutionPlan名称、全部原阶段及functions数组；每项交接该映像原命名callee的name和原NativeCode查询code。多个独立代码映像分别使用字节零相对IP，外层address_basis=each_image_relative，不伪造共同装载地址。需要更小范围时调用原stages()及所选NativeCode的查询。普通函数也由同一Emitter::finish生产范围，原COFF物理记录与已声明范围可用下述.5.3.1查询，完整生成类别可由下述COFF独立范围材料交接。

ModuleCode::code_json直接查询原所选材料，不读取源码／对象或创建VM；ProgramImage::code_json的module_code消费该结果，无模块为null。原address_basis=section_relative、sections的section_index／name／flags／byte_length／bytes／relocations及symbols不变。Section目标保留节索引与偏移，Import保留library／symbol；完整加数所在字节保持，不把占位值写成最终地址或把已有符号表补成源码私有名。

function_tables按原.pdata节和每12字节记录顺序返回section_index、offset_bytes、record_bytes、status及begin／end／unwind。各地址字段交回addend、原relocation_index、status、position；仅类型3 ADDR32NB按原32位RVA加法交接可确定的节内位置。没有重定位、其他类型及Import目标分别明确未知；同节、顺序合法的begin／end为resolved，跨节为cross_section_range，其他为unresolved。unwind的位置独立交回，不解码为机器指令；真实格式、目标或范围错误直接失败。

各section的regions保留kind、offset_bytes、length_bytes、bytes及instructions；偏移和半开范围相对本节。原声明范围为runtime_function，function_table_index引用本次结果中的原记录；其余为unclassified且instructions=null。函数指令复用现有x64解码，IP相对本节字节零，不代表装载地址；解码失败保留节号、实际偏移／相对IP及原因。区域可因原声明重叠，不能相加推断唯一代码量；未知、数据或范围外字节不按flags／符号排序猜成代码。直接模块查询只需原ModuleCode；整体根和资源CPU映像仍分别遵循NativeCode观察材料规则，不改变执行字段、资源ABI或最后消费者。

`bc build`显式导出原`.x64`时同时写同名`.x64.json`，包含同一映像的分区、指令及原重定位；这次查询不改变发射字节、CPU／GPU执行路径或正常`bc -c`部署内容。普通`.reloc`导出仍沿原入口，JSON查询保留每个映像原本已有的全部重定位。

这些范围是compiler结果中的观察材料，不属于必要执行字段；无compiler的NativeCode不含tail_regions／tail_references／sources或查询／解码实现。现有执行编码的字节、重定位、stack_bytes、calls、function_end及unwind按原顺序保存，ABI不变；恢复执行映像后的compiler值没有尾部分区材料时，code_json明确报告缺失，不从源码、旧模板或可解码性重建。所有输出字符串归调用者，查询结束不持有代码、资源、线程或设备消费者。

### 普通CPU函数操作的实际发射范围

compiler角色的NativeCode.sources以CodeSources::Emitted可选持有operation_ranges，元素OperationRange包含start、end、includes_nested_operations及原OperationOrigin。范围由同次标量block或SIMD packet_block在实际操作发射前后读取原字节游标，统一由原Emitter::operation_range交接，不重走发射、复制IR或按汇编猜来源。普通application::emit_value及codegen::emit的并行主入口启用该材料；custom_reduce::emit的自定义集合八类入口也启用同一材料；其余内建集合发射器及完整生成职责继续原台账。材料完成时从Emitter移入同一NativeCode，只共享原SourceSite与GeneratedOrigin，不持有Inst、ValueCode、Stage、AST／World或运行资源。

NativeCode::code_json在原输出中增加operation_ranges：None为null，已采集但没有操作为[]，每项提供start_byte、end_byte、byte_length、includes_nested_operations及origin（原OperationOrigin JSON）。偏移相对该映像字节零，单位为字节且范围为半开；机器反汇编仍来自同一最终bytes。序列按操作发射完成顺序交接，实际嵌套操作先于包含它的父操作，范围可以包含／重叠；这不是前端IR编号、独立指令数或动态执行次数，不能求和计成本。零长度明确表示该原操作没有发射字节，其位置只是游标，不能当作可设置断点的指令地址。origin两项均为空仍表示真实范围的来源未知，与整份材料未保留分别表达。

并行主入口复用同一范围容器：标量尾部及不采用SIMD的实际IR由原block记录；SIMD临时Step直接借用对应原Inst，packet_block发射后在同一Emitter记录范围。Plan的执行kind、向量化条件、地址专用Index移除与机器字节保持；Step借用随临时计划结束，最终材料只持有原OperationOrigin。已移除的SIMD步骤没有发射范围，不伪造零指令；同一原操作可在SIMD及标量尾部各有实际范围。并行生成外层范围边界／选择／索引推进、四lane收集及帧包装沿原FunctionAbi交接，不把它们套成业务语句；tile包装及内部生成分别沿原实际发射者描述。

自定义集合的原custom_reduce::emit将初始化、循环体及末尾真实Inst依次交给原block，与普通函数／并行代码共用operation_range。Reduce／ReduceTail、Scan／ScanPrefix／ScanAdd、SegmentReduce／SegmentPrefix／SegmentFinish均保留实际范围，分段扫描复用原Scan路径；现有值核的语法及生成关系保持，原生成Inst无来源时仍为未知。最终材料只持有原OperationOrigin，临时kernel／Stage／IR不因观察延寿。真实IR范围不代替机器原语、帧／范围循环和摘要输出的完整生成职责，其余内建集合的机器来源仍待补齐。BLCR7／BLCF10字段编码与执行ABI保持，原材料没有采集的范围不能由新版本查询补造。

tile_publish与tile_atomic经原Builder::emit交接当前声明语句及实际定义／实例／内联上下文到TilePublish，不绕过该原来源生产者构造空Inst。CPU tile发布和tile集合的原Inst在phase计划中直接借用，临时计划不复制另一套指令类别／载荷；原活跃性沿同一live_before与Inst.results处理。实际publish_values／tile_collective生成前后复用已有operation_range交接整条原IR的真实范围及OperationOrigin，普通block仍走同一机制；源定义、语法、实例、内联及生成关系按原指令保留，未知不猜造。原IR范围不是每个共享片段、单位元、循环或填充的独立生成描述，已交接的机器来源包括发布内部地址、加载、移位、存储、集合位置、单位元、独占写出、累积回边和归约填充。IR借用随阶段消费结束，最终材料只留不可变来源；CPU指令、GPU SPIR-V生成、原phase／liveness／布局／完成保持，格式沿现行BLCR7／BLCF10。恢复只交接所供材料已有记录，不因新查询端而补造范围。

当前范围仅描述已交接的实际IR操作发射；参数home、栈保存／输入／结果搬移、退出以及尾部探栈／字面量分别由原ABI与尾部材料交接，不把未命中范围自动解释为生成代码或已消除。原reads::place移动完整Inst，范围对应实际发射顺序；CPU纹理降低的新指令沿原采样共享来源与cpu_texture_sample生成职责，普通函数的同次操作范围已交接该材料。执行编码不保存来源，compiler角色恢复执行映像时sources为Unavailable；BLCR7将已保存的操作来源恢复为同一sources中的只读SavedSources；BLCF10沿原COFF布局保存／恢复这些已有操作范围及原OperationOrigin，不自动重编。

### 普通CPU函数ABI参数的实际发射来源

compiler角色的NativeCode可选持有function_abi：FunctionAbi只包含原函数definition、实际instance与平铺ranges，不持有FunctionBody、CallParameter类型合同、分配器或运行资源。原emit_function提供已有SourceOrigin和真实实例名，普通ValueCode发射没有这些事实则分别为None；不从首条操作或相邻源码补造。材料由原emit_value在实际生成点暂存，完成时移动至同一NativeCode；ParameterOrigin沿原参数不可变共享，生命周期随最后代码事实消费者。

code_json的function_abi未保留时为null；已采集但没有当前已交接ABI步骤时仍为对象、ranges为[]。每个AbiRange输出start_byte／end_byte半开字节范围、byte_length及实际职责kind。范围相对同一映像字节零，来自原发射前后游标；零长度表示该步骤没有新增字节，不能作为断点地址。operation_ranges继续只表示实际IR操作，不因ABI观察加入业务Inst或强制变量保活。

参数相关的三种AbiRangeKind分别是HiddenResultHome、ParameterHome和ParameterInput，JSON的kind为hidden_result_home、parameter_home、parameter_input。hidden_result_home只交接真实abi_slot，不伪造源参数；其余两种还交接原CallParameter列表中的parameter_index及原ParameterOrigin JSON（未知为null）。abi_slot是Windows x64实际参数槽序号，parameter_index不是源参数索引，隐式Frame和生成参数沿原ParameterOrigin分别说明；间接结果地址及零尺寸按值参数的省略会改变机器槽与逻辑参数的对应；零尺寸值不占ABI槽或生成home／input机器范围，逻辑IR参数身份保持。寄存器参数home最多覆盖原前四个实际槽，栈上传入的后续槽没有此步骤；parameter_input覆盖原读取循环中的每个实际机器参数，包括间接聚合与Reference，输入逻辑寄存器和类型继续查询同一FunctionBody.parameter_inputs，不复制到本材料。

栈维护沿实际生成点继续交接scratch_save／scratch_restore、frame_allocate／frame_release及nonvolatile_save／nonvolatile_restore。frame_allocate记录原allocate_frame调用和frame_bytes，即使原调用未发射字节仍保留零长度范围；frame_release只在原真实释放分支出现，frame_bytes为该分支原字节数。scratch两类仅在原分支实际执行时出现；nonvolatile两类来自原preserve_registers调用，未保存寄存器时可以是零长度。实际寄存器、机器指令、栈布局及展开继续由原所有者和同一最终代码说明，不复制保存清单或分配器。大帧分配范围可包含原探栈调用，其尾部辅助来源仍按独立叶推进。

结果交接分别为result_address_save、indirect_result、float_result及integer_result，由原实际结果分支直接选择。前两类携带stack_offset_bytes，表示该生成点相对已分配帧RSP的隐藏结果地址保存槽，不是参数home槽、源字段或资源地址；保存与间接输出分别记录各自真实范围。float_result沿原浮点返回，integer_result覆盖原整数及打包聚合结果；没有结果则没有输出记录。原结果类型、分量布局和输出寄存器继续查询FunctionBody，不保存结果运行值或伪造return语句来源。

共同末尾机器返回由AbiRangeKind::FunctionReturn交接，JSON kind为function_return，只记录原emit_value在结果与栈恢复后实际发射RET的范围；function_end继续来自同一原主函数末端，不含尾部辅助或字面量。职责只说明这条真实机器返回，不伪造源码return、不解释为成功、失败或最后资源消费者完成；源内提前return形成的原Scope／Exit仍随operation_ranges交接，不创建另一退出目标、图或清理链。

原可失败函数成功出口的状态清零由AbiRangeKind::FallibleSuccessStatus交接，JSON kind为fallible_success_status；范围只包含原emit_value在fallible分支发射的RAX置零指令，表示没有拥有错误，不是业务结果或共同机器返回。无失败函数不生成此职责。真实错误值装载和原退出跳转继续沿Raise的operation_ranges及其原触发来源表达，不重复生成ABI标签、退出目标或来源图；失败跳转绕过成功状态清零，原错误和清理语义保持。当前已交接参数home、输入搬移、栈维护、帧内结果地址保存、结果输出、可失败成功状态及共同机器返回，不能把未命中范围的字节自动归类。必要执行编码不保存来源，compiler角色执行解码后sources为Unavailable；BLCR7恢复原ABI只读事实；BLCF10沿同一Object／Section显式保存与恢复原ABI事实，不自动重编或从执行字节补造。

### 原生尾部目标及真实rel32字段

compiler角色的NativeCode可选持有tail_references，元素TailReference只含target与rel32_fields。原finish在同一次探栈辅助发射时取得原probe字段位置和真实辅助起点；在同一次字面量发射及patch之后直接移动原references Vec，原位模式／宽度去重、数据排序和字节不改变。不复制字面量表、IR或来源图，不重放发射、重新扫描位模式或把最终汇编猜成原发射引用。原tail_regions继续拥有目标种类与区间，不在新记录保存第二份分类。

code_json的tail_references未保留时为null，已采集但没有相应引用时为[]。每条仅输出target_offset_bytes及rel32_field_offsets_bytes：都相对同一代码映像字节零，目标是原辅助／字面量起点，每个字段是原四字节、有符号、小端PC相对位移的位置；原目标等于该字段末端加所编码的位移。字段位置不是指令地址或断点位置，也不是源操作编号、地址空间中的任意指针或动态访问次数。共享目标可以有多个真实字段，对齐区间不被伪造为目标或指令。

普通函数的使用来源由字段所属的原operation_ranges及function_abi范围说明；多个父子范围如实保留，不强选唯一语句。字面量数据只说明实际存储，原值来自源字面量还是编译期生成仍由原IR说明；仅凭池中位模式不能猜源数值类型或最初声明，确切值与合同继续由原IR查询。探栈调用可沿原FrameAllocate范围理解生成职责；辅助代码和调用代码分别说明。其他CPU并行／SIMD／集合入口复用同一finish交接物理引用，并不因此已有其完整源码关系，原未交接来源保持未知。

材料从原finish局部容器移动至同一NativeCode，原字面量references Vec直接转移；不持有Emitter、Frame、字面量拥有表或运行资源，随最后代码事实消费者结束释放。必要执行编码不保存tail_references，compiler角色执行解码后为None；BLCR7保存／恢复同一可选引用；COFF原Function Layout按值接收同一可选引用，并沿BLCF10保存／恢复，查询不自动重编或从指令补造。必要执行编码、执行ABI与实际字节不变。

### IR操作的语法来源

原`Inst`为持有`InstKind`的操作节点；原枚举变体移到`InstKind`，`Inst::new(kind)`构造没有语法来源的节点。compiler功能提供`Inst::source_site()`，可选只读引用指向`SourceSite`；`origin()`借用原`SourceOrigin`，`span()`返回该次语法处理的范围，`SourceSite::syntax()`的枚举为`Statement／Expression／Place／Parameter`；原直接发射的source_site使用前三类，形参生成触发可用Parameter。普通运行角色不包含这些来源字段、类型或查询输出；B语法、C ABI、必要执行格式和原优化选择保持。Rust调用方构造／匹配IR时使用原节点的kind，不能继续将Inst当操作枚举。

Builder在实际语句、表达式、返回值表达式和位置处理期间交接所消费的Span；子表达式结束后回到原父上下文，实际内联期间使用原被调定义快照，返回后恢复调用者。原code的每个操作持有自己的可选语法来源，原嵌套块、克隆和移动沿同一节点交接；同一Builder定义和原Source文本共享不可变Arc，不建立按操作序号对齐的来源表或保留AST／World。语法范围表示驱动本次发射的上下文；参数设置、运行框架和后端新合成操作未取得语法来源时为null，不能从根范围补造。

`SourceSite::ir_json()`的kind为source_syntax，syntax为statement／expression／place／parameter；parameter只说明原参数起始令牌，不是新的B语法，definition沿原source_definition输出同一原定义及快照；其余start_byte／end_byte、span_unit／span_range、start／end、column_unit和position_base与原定义查询采用相同单位，但范围属于本次语法上下文。现有compiler角色的每个IR操作JSON增加source_site，对没有材料的操作输出null；关闭compiler时不输出该字段。只在显式查询进行摘要、行列和JSON计算，已经取得的IR不重读后续磁盘文件。

语法位置、实际实例／内联展开点、parallel阶段、集合调用及内部值核入口已交接；域展开与已编译域调用点沿下述domain_path交接；阶段索引／空前缀生成原因沿generated_origin交接；Frame／域包装分类继续见Issues C-021.7.9.7.2.4.5／.6。优化器原克隆、移动和就地修改保留节点；新构造操作没有自动继承来源，完整合并、折叠、消除和结构重写仍沿.7.3实施。source_site为null只说明此操作没有已交接的语法材料，不等于没有源语义、已消除或机器代码无来源；不得据此冻结优化或保活操作。机器指令来源与可选产物保存继续沿.7.4至.7.6，当前IR语法来源不能代替这些关系。

条件输出的字节范围属于原合同定义。原Contract::Declaration借用Definition，直接取得同一原合同列表及语法上下文；outputs::expression在convert及builder.expr期间成对使用合同span与其来源。相同Definition保留已有实例／内联／域上下文，跨定义采用原定义能提供的上下文，未传具体Instance不猜补；已绑定Signature沿原真实调用处span／source保留调用位置上下文。求值完成或返回错误后，原Builder的来源定义、上下文、语法点和原env恢复，其他调用／值／Proof保持。

外部声明或已编译提供者缺少真实实现来源时保持null，不能把声明的索引解释成调用者源码，亦不在位置查询截断或改写越界范围。真实普通B定义的条件仍保留其SourceSite，不通过清空全部来源消除错配。外部具名声明自身沿下述SourceDeclaration交接，声明与提供者实现分开。


#### 实际实例与内联展开点

compiler功能的`SourceSite::instance()`借用实际`generics::Instance.name`，为可选字符串；原模块／原函数名继续在`origin()`，不能用它们代替特化身份。普通函数、并行阶段及集合内部值核的原生产者直接交接当前实际实例；阶段参数的临时表达式也沿同一Instance交接，不从最终Program名或解析某个字符串猜补。模块常量／类型整数式等没有实际普通实例的求值入口为null，其内部真实展开的函数仍按实际特化交接。

`SourceSite::inlined_at()`借用`InlineSite`切片，按最外层调用者到最内层调用者排列。当前操作自己的实例在instance，数组中的每个元素是原inline_instance入口实际消费的调用者定义、实例及Span（当前普通调用通常是调用标识符范围，静态引用按原引用范围，不推测扩大为整条调用表达式）；同一被内联实例在不同调用点分别取得自己的路径，展开结束后恢复调用者。`InlineSite::origin()`返回可选原`SourceOrigin`引用，`instance()`返回可选实际调用者名，`span()`交回该次展开点的原范围；这三个getter只借用／复制原事实，`ir_json()`返回独立JSON。

每个source_syntax对象增加instance、inlined_at_order=outermost_first与inlined_at数组。每个展开点kind为inline_site，definition为调用者原source_definition或null，instance为实际调用者实例或null；start_byte／end_byte、span_unit／span_range及行列单位沿原语法范围规则。缺少调用者源快照时保留原入口交接的字节范围，但start／end位置为null；已知实例与后续已知展开点继续保留，不猜补缺失定义或丢掉整个路径。空数组表示本操作没有普通函数／cooperate的内联展开前缀，不表示运行时没有调用。

实际非内联普通调用不增加内联帧，其被调函数IR以自己的真实实例作为根；inline_site是编译期实际展开点，不是机器CALL、硬件现场或运行期调用栈。阶段展开保存当前阶段所属函数实例；集合调用自身另沿下述operation_source交接；集合值核入口／引用沿下述entry_origin交接；外层域调用点另沿下述domain_path交接，生成分类仍沿.7.2.4.4至.6继续实施；inline数组不冒称域调用图。优化合并／消除与机器地址继续沿.7.3至.6，不从剩余寄存器或新构造操作推测丢失关系。

同一展开体的语法点共享不可变实例名和调用点列表，原Source文本继续共享Arc；每次真正展开时只把现有路径及本次原调用点交给新上下文，不保留Builder、AST、World或另建来源表／调用图。资料归原IR并在最后消费者后释放；显式查询才进行摘要、行列和JSON计算，磁盘源码后续变化不改变已取得的根／调用点快照。普通调用、内联选择、合法程序、优化、C ABI及必要执行编码均不因这些观察事实改变。

#### 域展开路径与已编译边界

compiler功能的`SourceSite::domain_path()`借用`DomainSite`切片，按最外层调用者到最内层排列，与`inlined_at()`独立。原域调用点直接保存调用者原定义／实例、Expr.span、实际特化目标及转换种类；目标来自原generics::Instance.name，不解析最终Program名或原生符号猜补。原parallel阶段、普通IR语法点及集合值核的引用／捕获在原SourceContext共享同一路径；值核内部普通内联继承域路径，同时独立记录其真实内联点。根入口未经过本次域转换时数组为空，空数组不表示运行时没有调用。

`DomainSite::origin()`返回可选调用者SourceOrigin，`instance()`为可选原调用者实例，`span()`交回原调用Expr范围；`target()`借用实际目标实例，`transition()`为DomainTransition::Expanded或Compiled。ir_json的kind为domain_site，definition／instance分别为原调用者定义／实例或null，target为原目标，transition为expanded或compiled；字节范围、行列与缺失定义时的位置空值沿原inline_site规则。当前普通调用Span通常是原标识符范围，不扩大为整条调用或参数列表。每个source_syntax增加domain_path_order=outermost_first与domain_path数组，已有instance和inlined_at保持各自原语义。

expanded表示该源码域实际展开入当前IR；目标自身的原语句仍指向自己的定义／实例，外层点不替代它。compiled表示本次编译在已编译域边界留下原生调用；原DomainCall将同一调用SourceSite交给build_domain的原Call指令，末尾域点保留compiled目标，SourceSite自身仍属于调用者表达式。原CallTarget继续交接实际CPU／GPU原生符号，不能把逻辑目标名字当最终地址或目标实现源码，也不在已编译边界伪造内联。没有库源码仍沿产物接口编译；不在本次IR中的提供者实现不能从调用者补造。

列表和Source快照随原Definition／Stage／Inst／ValueCode不可变共享，Stage克隆共享同一路径，原IR最后消费者后释放；已取得IR不重读后续改写／移走的文件。只有查询承担摘要、位置和JSON成本；关闭compiler时没有这些类型、来源字段或JSON。路径不保留AST／World、复制调用图／执行图，或拥有运行资源，不改变ABI、数值、存储、原分段与完成。阶段索引与clear_prefix通过下述generated_origin的trigger继承原语句／调用及域路径；Frame／参数和分段提交未交接的材料仍明确缺失，其生成职责分别沿.2.4.5／.6继续实施。这里只描述实际保留IR的已交接路径，不记录没有IR的全部编译历史；融合后的单个阶段来源不是全部贡献者，完整合并／消除与机器范围仍归.7.3及后续叶项，不冻结优化或保活操作。

#### 集合调用的原算子来源

compiler功能的`Stage::operation_source()`返回可选只读`SourceSite`，归同一个Stage所持`Operation`所有；`stage_ir`增加operation_source，按原source_syntax编码原定义快照、具体实例、语法类别及范围，材料缺失为null。内置／自定义归约、普通／分段扫描、分段归约、compact和stable_sort_by_key的原生产者交接真实调用Expr.span和当前实际Instance，类别为expression。当前解析器的调用Span通常为调用标识符；字节半开范围及Unicode标量行列据此输出，不推测扩大为完整参数列表，不改原诊断所消费的语句范围。两个相同目标的调用仍有各自原位置，根Program名不代替实际集合所属实例。

集合即使没有普通Inst，也能取得自身来源；project／identity／combine的ValueCode入口、bind静态引用及捕获关系另沿下述entry_origin交接，不能用此集合调用或内部一条指令冒充这些入口。当前不把域函数展开伪装成普通内联点；inlined_at为空仅表示没有已交接的普通函数内联前缀，域调用点另沿domain_path交接。parallel的原生产者也沿同一operation_source交接原Stmt.span及实际Instance，类别为statement；当前Stmt.span通常覆盖parallel关键字，不扩大为完整语句。原整域源码与CPU应用提取路径均保留原位置；提取前的原Builder将不可变SourceSite交给同一Generated定义，后续阶段及真实克隆语句沿锚点共享原定义、原调用方实例和SourceContext。生成Program／FunctionBody的source_origin仍为null，原生成执行名称不写进语法来源实例，不伪造源码函数；其中真实内联仍记录真实调用者及目标。原clear_prefix及普通CPU发射所用合成Map没有本项语句／集合材料时仍为null，阶段索引与空前缀生成原因沿下述独立generated_origin交接，Frame／参数操作仍由.2.4.5／.6交接；域展开路径沿下述独立事实保持。融合后当前Stage的单个operation_source只表示原保留阶段的来源，不是所有被融合parallel的完整来源；多对多、重写和消除继续归.7.3.2，不改变融合或保活指令。

原Stage克隆共享同一不可变来源，Source文本随原快照共享，原IR消费者结束后释放；查询返回借用或独立字符串，不保留AST／World、注册表、运行图或额外状态。后续磁盘源码改变或移走不改变已取得材料。compiler专用Stage有内部来源字段，Rust消费者沿编译返回值及原Stage克隆取得，不依赖旧结构字面量／布局。关闭compiler时该字段及operation_source JSON均不存在；原ExecutionStage、BLRI必要执行编码、C ABI、访问、数值、调度和优化不因本项改变。优化多对多关系及CPU／GPU机器映射继续沿原叶实施。

#### 原IR生成原因与触发语法

compiler功能的`Inst::generated_origin()`、`Stage::generated_origin()`、`ValueCode::generated_origin()`及`Program::generated_origin()`借用同一节点持有的可选`GeneratedOrigin`；reason()交回GenerationReason，anchor()借用可选GenerationAnchor。trigger()保持原语法查询语义，返回Syntax锚点或UnaryConstantFold的真实表达式SourceSite；隐式定义实例及多输入变换锚点由anchor()取得，不能把trigger()为空解释为没有生成材料。ir_json输出kind=generated_origin、reason及trigger，trigger按自己的kind交接实际锚点，没有材料时为null。每个compiler指令、stage_ir、value_ir和program_ir交接generated_origin，没有本项材料为null；关闭compiler时这些类型、字段、getter及JSON均不存在。

GenerationAnchor::Syntax持有原SourceSite，JSON沿原source_syntax保持字节范围、定义快照、具体实例及域路径；Definition持有可选原SourceOrigin和实际Instance.name，JSON为kind=definition_instance，definition为原source_definition或null，instance保持实际入口名。GenerationAnchor::syntax()只借用实际语法点，Syntax／Definition、UnaryConstantFold、ModuleConstantEvaluation及StageConstantEvaluation按各自锚点的明示语法／定义和实际名称提供origin()／instance()；DomainCallLiteral的单源查询仅借用明确call成员，evaluation独立读取；ir_json()交接同一事实。定义实例只描述真实生成上下文，不伪造用户表达式、逐指令源范围或运行时刻；缺少原定义时仍保留已知实际实例，不根据最终程序名猜补。当前parallel_index及clear_prefix继续使用Syntax，原trigger()借用关系和JSON保持。

当前真实生成原因及其消费如下；GenerationReason公开枚举增加变体时，Rust SDK的穷尽匹配需要相应适配，按版本规则处理兼容性。B语法、资源ABI和必要执行编码不因此改变。

| Rust原因 | JSON reason | 原生产职责 |
|---|---|---|
| ParallelIndex | parallel_index | 原parallel索引建立 |
| ClearPrefix | clear_prefix | 原空前缀阶段与清零操作 |
| ApplicationEntry | application_entry | 原Application.body隐式入口包装 |
| ApplicationEntryCall | application_entry_call | 原包装调用实际选中B入口 |
| FrameErrorReceiver | frame_error_receiver | 原错误接收函数地址 |
| FrameErrorContext | frame_error_context | 原接收器上下文值 |
| FrameCreate | frame_create | 原blink_create调用 |
| FrameReadiness | frame_readiness | 原地址／零值／比较及就绪If，指令种类各自保持 |
| FrameFailureExit | frame_failure_exit | 原创建失败分支的退出值 |
| FrameDestroy | frame_destroy | 原正常路径blink_destroy调用 |
| DatasetAllocate | dataset_allocate | 原dataset构造的allocate_<type>调用 |
| DatasetInitializeCursor | dataset_initialize_cursor | 原游标准备零值及起始StoreLocal |
| DatasetInitializeCondition | dataset_initialize_condition | 原循环LoadLocal和Less比较 |
| DatasetInitializeCall | dataset_initialize_call | 原普通B、已编译或外部C factory调用 |
| DatasetInitializeWrite | dataset_initialize_write | 原写入零offset和元素StoreIndexed |
| DatasetInitializeAdvance | dataset_initialize_advance | 原递增常量、IAdd和StoreLocal |
| DatasetInitializeLoop | dataset_initialize_loop | 原初始化Loop本身 |
| DatasetBindSlots | dataset_bind_slots | 原dataset新绑定的句柄／长度局部StoreLocal |
| DatasetAssignSlots | dataset_assign_slots | 原dataset重新赋值的句柄／长度局部StoreLocal |
| DatasetRelease | dataset_release | 原dataset最终存储释放Call |
| DatasetDropCursor | dataset_drop_cursor | 原拥有元素清理的长度游标StoreLocal |
| DatasetDropCondition | dataset_drop_condition | 原逆序清理的游标读取／零值／比较 |
| DatasetDropIndex | dataset_drop_index | 原逆序清理的减一值与ISub |
| DatasetDropElementRead | dataset_drop_element_read | 原逆序清理实际元素分量ViewLoad |
| DatasetElementDrop | dataset_element_drop | 原一般拥有元素的类型化清理Call；局部已知拥有树不产生该Call |
| DatasetDropAdvance | dataset_drop_advance | 原逆序清理回边StoreLocal |
| DatasetDropLoop | dataset_drop_loop | 原拥有元素逆序清理Loop |
| UserOwnedDrop | user_owned_drop | 原owned<T,F>实际静态析构Call |
| ConditionalDropBranch | conditional_drop_branch | 原条件拥有清理的外层If |
| ConditionalDropCondition | conditional_drop_condition | 原Presence条件准备及其内部短路操作 |
| VariantDropBranch | variant_drop_branch | 原活动enum／Result清理的选择If |
| VariantDropCondition | variant_drop_condition | 原清理选择的判别常量及比较 |
| DatasetResizeFill | dataset_resize_fill | 原resize填充值进入调用暂存的StoreLocal |
| DatasetResizeCall | dataset_resize_call | 原Frame扩容及存储版本交接的resize Call |
| DatasetAddress | dataset_address | 原Frame地址请求的bytes常量及实际address Call |
| DatasetStorageAddress | dataset_storage_address | 原资源句柄进入地址型借用ABI的storage_address Call |
| DatasetRootAddress | dataset_root_address | 原U64根拆分或句柄地址请求的表示准备指令 |
| DatasetOffsetAddress | dataset_offset_address | 原双字地址与索引／步长／字段偏移的寻址准备指令 |
| DatasetWriteAddress | dataset_write_address | 原CPU句柄写分支的写区偏移及请求准备指令 |
| DatasetAccessLayout | dataset_access_layout | 原CPU访问的声明布局、读区请求及公共返回零索引准备常量 |
| DatasetFieldLayout | dataset_field_layout | 原资源引用的字段偏移、声明步长常量及实际字段偏移加法 |
| DatasetProjectionAddress | dataset_projection_address | 原CPU dataset构造view时投影地址准备的零索引常量 |
| DatasetUpload | dataset_upload | 原upload经所有权／借用／源位置检查后生成的Frame上传Call |
| DatasetDownload | dataset_download | 原download经所有权／借用／源位置检查后生成的Frame下载Call |
| ApplicationDomainCall | application_domain_call | 原应用将Frame与实际参数交给已编译整域入口的Defined Call |
| DomainEntry | domain_entry | 原build_domain形成的整域包装ValueCode入口 |
| DomainCallLiteral | domain_call_literal | 原build_domain将Root::Literal实参分量准备为Constant |
| DomainPartCall | domain_part_call | 原emit_domain_part交接实际Stage子区间的运行Call |
| DatasetCallArgument | dataset_call_argument | 原native_call将Stage Dataset准备为实际借用实参的四类指令 |
| DatasetBorrowStrideCast | dataset_borrow_stride_cast | application_reference_value的实际步长拓宽，以及application_stride_parts将view实际步长拆成低／高U32的表示转换 |
| ArrayParameterStore | array_parameter_store | 原值形参数组／含数组聚合物化到局部存储的StoreLocal |
| IntegerDivisionPair | integer_division_pair | 原同一直线块整数商／余数实际配对合并 |
| ExpressionReuse | expression_reuse | 原ExpressionKey命中后实际复用已有结果 |
| LocalReadForward | local_read_forward | 原LoadLocal实际被已知局部值替代 |
| IndexedStoreReadForward | indexed_store_read_forward | 原索引Load实际被先前Store写入的值替代，材料交给原保留Inst |
| IndexedReadReuse | indexed_read_reuse | 后续索引Load实际复用先前Load结果，材料交给原保留Load |
| UnaryConstantFold | unary_constant_fold | 原普通已知值一元计算逐分量成为Constant |
| ModuleConstantEvaluation | module_constant_evaluation | 原模块const求值中的已有变换事实随普通常量读取进入实际Constant |
| StageConstantEvaluation | stage_constant_evaluation | 临时阶段表达式已有求值事实随原Root及实际值IR交接 |
| DeclarationReference | declaration_reference | 真实具名声明与实际Call／FunctionAddress的绑定关系 |
| ContractCondition | contract_condition | 原具名声明的真实条款生成条件IR的关系 |

原parallel的Index由expand_stages交接parallel_index，触发与同一Stage.operation_source共享原Statement SourceSite；普通源域与CPU应用提取均沿原定义／实例，生成执行名称不成为原源码函数。这是并行索引建立的生成事实，不把它当作某次用户index()表达式。该Index的source_site仍为null，普通语句、表达式及位置的直接来源继续沿原source_site交接；Stage自身仍是原parallel语句，其generated_origin为null。

原clear_prefix调用由append::clear生成原Map、零常量和计数Store；同一Stage和两个Inst共享clear_prefix的GeneratedOrigin，trigger为真实调用Expr范围及当前实际实例／域路径。当前解析器的普通调用范围通常覆盖clear_prefix标识符；原诊断用语句Span保持，不扩大为未取得的完整参数列表。没有额外Index、另一个Stage或输出清空，原计数归零与空prefix关系保持；生成Stage.operation_source及两节点source_site仍为null，不伪造用户直接Map／赋值。

Inst::new继续构造没有来源材料的普通节点；原直接发射、原生域Call及其他Stage构造按其真实职责保留已有source_site／operation_source，尚未交接的生成原因不猜补。generated_origin为null只说明没有这份分类材料，不证明不是生成代码、已经消除或机器范围无来源。程序入口／根Frame、dataset分配、生成函数初始化、新绑定／重新赋值槽位、最终存储释放、拥有元素逆序清理、静态F、条件及活动变体清理和resize填充值暂存见下文；实际扩容Call来源见下文；寻址、参数、传输和发起仍归.2.4.5.5至.8，原生域包装归.2.4.6，不以已交接分类冒称全部框架生成。

材料沿原Inst／Stage／ValueCode不可变共享、克隆／移动及最后IR消费者释放，不保留AST／World、注册表、拥有表、执行状态或第二调用图；原Source文本及上下文共享，查询不重读后续磁盘文件，只有显式查询承担摘要／行列／JSON成本。不改变IR操作、寄存器、访问、计数、数值、调度、ABI或完成，不进入必要执行编码。优化器原节点保留／克隆继续携带材料；新构造、融合、重写及消除的完整关系仍归.7.3，不为来源保活操作、冻结优化或把单一保留材料当作完整多来源。

#### 整数商余配对合并的来源

原B值流优化把同src／IntegerDivisor的Quotient与Remainder配对后，合并Inst的generated_origin.reason为integer_division_pair，Rust枚举为GenerationReason::IntegerDivisionPair。anchor()取得GenerationAnchor::IntegerDivisionPair { quotient, remainder }；每一路OperationOrigin提供source_site()、generated_origin()及ir_json()，只借用或序列化原输入已取得的来源。JSON锚点kind为integer_division_inputs，两个成员各为kind=operation_origin、source_site及generated_origin，未知材料为null。输入先余数后商时，成员仍按商／余数角色排列；原生成来源若存在也按原不可变材料共享，不丢弃或猜补。

合并节点没有唯一直接语法来源，其source_site()为空；该多输入锚点的syntax()／origin()／instance()及外层trigger()均为空，不任取一路代表完整变换。两路SourceSite保留自己的真实定义快照、实际实例、inlined_at及domain_path。当前解析器对二元除法／余数Expr.span通常只覆盖左操作数起始token，例如value，不扩大为整段value/divisor或value%divisor；查询使用真实UTF8字节范围及原快照，不重读之后改写或移走的磁盘源码。

材料归原Inst并随原IR／Stage克隆、模块和SDK编译结果在最后消费者后释放，只共享来源，不保留旧输入Inst／IR、AST／World、运行状态或额外图。后续DCE把Both裁为单结果时，IntegerDivisionResultPrune.retained保持此前的配对来源；整条删除时，原EliminatedOperation将来源交给ValueCode／Stage及空Map后的Program。保留来源不保活旧指令，也不声称仍有两个输出。配对条件、寄存器／结果、数值和控制边界不变，compiler之外没有本类型／材料，必要对象与运行编码不变。其他复用、重写／消除及机器范围仍按原叶推进，不把本项当作完整优化历史。

#### 相同表达式结果复用的来源

原B值流优化在整数Divide键或普通Constant／Index／Uniform／BufferStride／Unary／Binary／Cast／Math／Select键真实命中并产生别名时，沿原保留结果Inst交接GenerationReason::ExpressionReuse，JSON reason为expression_reuse。GenerationAnchor::ExpressionReuse { retained, reused }的retained为保留操作原有的OperationOrigin，reused为各实际复用输入的ExpressionReuse；已有source_site及generated_origin不可变共享，未知材料为null，不根据函数名、最终形态或当前磁盘文件猜补。

ExpressionReuse::result()取得实际被复用的结果角色ReuseResult::Value／Quotient／Remainder；input_results()借用该输入操作在本次复用发生前实际产生的结果集合：普通纯值为[value]，单商／余数为[quotient]／[remainder]，Both为[quotient,remainder]。origin()借用该被删除操作的OperationOrigin，ir_json()输出kind=expression_reuse_input、result、input_results及origin。锚点JSON为kind=expression_reuse、retained和reused数组；它记录实际键相等及结果替代，不允许据此推定任意数值重排。输入结果集合不从先前integer_division_pair来源或最终Both／单结果反推；部分复用后可能再次配对，后续DCE也可能裁剪，保留事实不声称那些结果仍存在或已在运行中被消费。

原source_site仍表示保留操作自己的直接来源，不是全部复用点。多输入的trigger()及锚点syntax()／origin()／instance()为空，各路从retained和reused取得；这里锚点origin()和ExpressionReuse::origin()各有其上述明确语义。原实例、内联、域路径及源字节范围随各自SourceSite保持，多个删除点可归到同一保留结果，不强制一一映射。getter和JSON均沿已持有的快照查询，不重读后续改写或移走的文件。

本次值流调用只以原SSA号暂存尚待交接的材料，沿原完整IR以及归约公共入口／各原函数体归到真实保留Inst后释放；不会随Values克隆另一表达式权威，不让兄弟函数体复用未执行的定义。永久材料没有寄存器号、旧输入Inst／IR、AST／World、运行状态或另一图。来源随原Inst／Stage／ValueCode及模块／SDK编译结果共享，最后IR消费者后释放，不拥有运行资源。compiler以外没有这些类型、代码或材料，必要对象／运行编码和ABI保持；原别名键、配对／裁剪、控制、数值及完成不变。后续DCE照常删除，删除历史、结构及机器映射仍按原独立职责推进，不为观察保活或把本项当作完整优化时间线。

#### 局部读取到实际定义与输入的来源

原B值流优化仅在LoadLocal命中原locals、实际写aliases并消除该读取时交接GenerationReason::LocalReadForward，JSON reason为local_read_forward。写入／失效、分支／循环及继续关系沿原算法，材料不增加复用、保活操作或重排许可。普通指令定义与直接ValueCode输入按各自真实拥有者表达，不为没有定义Inst的输入伪造指令。

替代值由普通Inst定义时，原Inst.generated_origin采用GenerationAnchor::LocalReadForward { retained, reads }。retained为原OperationOrigin，reads的LocalReadForward逐项提供result_index()、result_count()、result()和origin()：序号及数量取本次值流交接时该定义实际Inst::results顺序，序号零基；ReuseResult::Value表示普通分量，Quotient／Remainder另区分整数结果。多结果Call可以各有自己的序号；后续配对、裁剪及删除不由这些字段反推，它们不是最终机器寄存器或运行次数。JSON锚点kind为local_read_forward、result_index_base为0；各读取kind为local_read_forward_input，含result_index、result_count、result及原origin。原Inst.source_site仍描述保留操作自己的直接语法，读取来源分别沿origin取得。

替代值直接为原ValueCode.inputs时，原ValueCode.generated_origin采用GenerationAnchor::LocalInputForward { retained, inputs }，retained保留可选原根GeneratedOrigin，原entry_origin独立保持。InputReadForward::input_index()引用该ValueCode实际inputs的零基位置，reads()借用各被消除读取的OperationOrigin，ir_json()输出kind=input_read_forward、input_index及reads。锚点JSON为kind=local_input_forward、input_index_base=0、retained与inputs；输入没有伪造定义Inst、永久Reg副本或另一参数槽位映射。FunctionBody::parameter_inputs仍直接借用同一输入列表；空分量B形参不制造输入，B形参序号和IR输入序号分别表达。

CPU函数在原Frame判定前后可以再次优化。原finalize实际移除Frame参数及inputs前缀时，同步调整本输入引用并移除已不存在输入的材料；使用原不可变共享语义保持旧快照，不以来源保留无消费Frame。普通无Frame标量的实际输入索引可以为0，保留Frame时普通值可位于实际前缀之后；以真实列表为准，不从参数名字或旧编号猜测。根generated_origin出现本理由仅说明发生了输入读取替代，不把普通源函数说成合成函数或应用入口。

两类锚点的单源trigger()、syntax()／origin()／instance()均为空，各路从明确成员取得；原读SourceSite及GeneratedOrigin共享已有定义、实例、内联／域路径及真实字节范围，缺失为null。查询不重读后续改写或移走的文件，不保存旧Inst／IR、AST／World、运行拥有表或第二状态图；同一次值流仅用原SSA键暂存待交接材料，交回原拥有者后释放。材料随原Inst／ValueCode／Stage、模块及SDK编译结果在最后IR消费者后释放，compiler以外没有这些类型／代码，必要对象／运行编码及ABI保持。归约降层新增输入读取来源按后节交接，写失效及控制合流的完整事实仍沿.3.4至.3.8实施；无材料不证明没有发生传播，静态替代不证明已运行，不把前两叶当作全部局部值历史。

#### 索引Store写入值到保留指令的读取来源

原Values.memory在Store时临时保留同一Reg及该Store的OperationOrigin；Load建立的条目不伪造Store来源。后续同binding／index／word的Load真实被原值替代时，原Reuses将Store及被替代读取的不可变来源交回实际定义该值的Inst。原写效果／别名／控制范围不改变，不记录失效历史、物理地址或永久SSA键；写入值的生产者、Store与后续读取是三个不同接点。

原Inst.generated_origin使用GenerationReason::IndexedStoreReadForward及GenerationAnchor::IndexedStoreReadForward { retained, reads }。retained保留该值生产者原有的OperationOrigin，原source_site仍是它的直接语法。reads中的IndexedStoreReadForward提供result_index()／result_count()／result()、store()和read()；前者引用本次交接时Inst::results的零基顺序和ReuseResult语义角色，后两者分别为实际Store和被替代读取的OperationOrigin，不把Store说成值定义或先前Load。

JSON根reason和锚点kind均为indexed_store_read_forward，锚点含result_index_base=0、retained和reads；各项kind为indexed_store_read_forward_input，字段为result_index、result_count、result、store和read。多源锚点的单源trigger()／syntax()／origin()／instance()为空，各路从对应成员读取；未知来源仍为null。Source及既有生成链不可变共享，后续优化可以沿原材料继续交接，不保活DCE操作，不保存旧IR、AST／World、运行状态或第二张内存表。

此项覆盖写入值由保留Inst定义的范围；先前Load定义的复用按后节交接。普通CPU函数的资源参数使用LoadIndexed／StoreIndexed地址访问，不能据此推定发生原Values.memory中Load／Store的输入转交。归约输出Store由native/custom_reduce在kernel值流之后生成，不重新进入此前的输入读取归集；当前正式路径没有已确认的两类Store值到输入事件。已有LocalInputForward及ReductionInputForward继续表达各自真实局部读取；不新增预设输入锚点，也不把当前构造方式写成语言限制。没有此材料不证明不存在读取替代，更不证明运行时地址、数值、次数或完成。材料随原Inst／Stage／ValueCode及最后IR消费者释放，compiler以外无这些类型／代码，必要对象／运行编码及ABI保持。新增公共枚举分支属于compiler SDK源码接口演化，软件版本与执行ABI分别表达。

#### 先前索引Load的读取复用来源

原Values.memory由真实Load建立的无Store来源条目仍指向该Load的dst；后续Load命中并消除时，原Reuses将被替代读取的OperationOrigin交回实际保留Load。原memory键及写／别名／控制处理不变，不新增装载、复用许可、失效历史或地址表；Store值转交另用前节的独立原因，不混为先前Load定义。

原Inst.generated_origin使用GenerationReason::IndexedReadReuse和GenerationAnchor::IndexedReadReuse { retained, reused }。retained是原保留Load的OperationOrigin，reused是各实际被替代读取的OperationOrigin；原source_site仍是该Load自己的语法。原Load的dst、binding、index、word及所属寄存器表解释本次保留逻辑分量，材料不保存优化前SSA编号或运行地址。一个float3由原三条标量Load分别交接，不把三个分量混成单个读取，也不将本例宽度作为语言限制。

JSON根reason及锚点kind均为indexed_read_reuse，锚点含retained和reused；每个成员沿原operation_origin编码SourceSite及已有GeneratedOrigin，缺失材料仍为null。SDK通过anchor()取得多源成员，单源trigger()／syntax()／origin()／instance()为空。来源及实际实例、内联／域路径和字节范围不可变共享；查询只消费既有材料，不重读磁盘或保存旧IR／AST／World。这里记录本次交接时的保留Load，后续变换仍可把它作为既有来源交接，不能因材料仍在而断言最终仍有该Load或已经运行；DCE不为来源保活操作。

当前simulation.bodies的integrate／step入口可查询到position读取的复用；能量函数内同一数值的复用不自动等于再次读取dataset，本次energy入口没有该材料。compiler以外无这些类型／代码，必要执行编码及资源ABI不变；公共枚举增加变体按原版本规则处理。材料沿原Inst／Stage／ValueCode及最后IR消费者释放，不拥有运行资源或新增调度／完成／释放链。

#### 未消费普通值结果的删除来源

同一B消除算法在非Atomic／Append／Print／Call的指令确有结果、且所有结果均不在原needed集合时删除整条普通值操作；结果枚举只交“有结果／有活跃结果”两个32位事实，是否继续由同一B续接决定，不生产未消费的完整计数；单／多结果均沿原规则，整数除法的专门裁剪、私有写及结构删除分别归原职责。原ValueCode／Stage的generated_origin交接GenerationReason::UnusedOperations及GenerationAnchor::EliminatedOperations { retained, operations }，retained保留可选原根GeneratedOrigin，ValueCode.entry_origin及Stage.operation_source独立保持。原优化、活跃性、副作用、数值及完成不因材料改变，没有删除时不增加包装。

operations中每个EliminatedOperation代表本次消除真正删除的一条IR操作。ValueResults变体保存有结果操作的result_types及可选integer_division_results，result_types()返回Some，按当时Inst::results顺序交回实际Scalar列表；DirectPrivateWrite／IndexedPrivateWrite变体没有结果值，该getter返回None，私有写不伪造结果列表。origin()借用各自同一操作的OperationOrigin。它不保存旧Inst、操作数、常量值、旧Reg或物理位置，不将一条IR操作扩大为整个源表达式、实际运行次数、最终设备指令数或性能收益；一次源操作可能降成多个逻辑分量，多个来源也可能已合在一个被删除操作的已有来源里。递归沿Scope／If／Loop把本次删除直接交给同一原根输出Vec，列表顺序仍是该次遍历的收集顺序，不是运行时间线或稳定身份。

GeneratedOrigin的JSON reason为unused_operations，锚点kind为eliminated_operations，字段为retained及operations；ValueResults的kind为eliminated_value，字段为result_types、integer_division_results及origin，类型拼写与本次IR寄存器类型相同。普通值的integer_division_results为null，整数除法删除时为原商／余数角色数组，见下节。多源单一trigger()／syntax()／origin()／instance()为空，各来源从明确成员读取；未知来源仍为null。Frame输入前缀真正移除时，原remove_input_prefix沿保留根继续调整LocalInputForward输入引用，旧共享快照和已取得删除来源保持；不改原Frame判断或ABI。

材料仅compiler，沿原ValueCode／Stage及编译／模块／SDK快照在最后IR消费者后释放，不保活操作、槽位或阶段，不持有AST／World、运行资源或另一活跃性表，不进入必要对象／执行编码。整个空Map阶段移除后的来源由原Program沿C-021.7.9.7.3.2.2继续交接；没有材料不证明操作未发生删除。SDK新增公开枚举／类型属于compiler源码接口演化，软件版本与执行ABI分别表达。实际入口继续使用bc ir的普通函数、应用和程序查询，不增另一调试命令。

已有实际查询入口：

```text
bc ir crates/blanguage/src/bootstrap/parser.b -I crates/blanguage/src --module
bc ir sample/applications/datasets.b -I sample --entry main
```

这些查询返回当前编译结果中的删除材料；外模块本体、已经结束的临时检查根与所请求模块分别归其真实拥有者，不把同一源码范围的多次检查计成该结果的多次删除。

#### 整数商余结果裁剪的来源

同一B消除算法只按实际needed筛选IntegerDivisionResult的quotient／remainder。Both真正改成单商或单余数时，同一保留Inst.generated_origin使用GenerationReason::IntegerDivisionResultPrune及GenerationAnchor::IntegerDivisionResultPrune { retained, input_results, output_results }。retained是本次裁剪前的OperationOrigin，原source_site不变；两个角色列表直接来自本次原结果及保留结果的IntegerDivisionResult::outputs，成员为IntegerDivisionOp::Quotient／Remainder，顺序为商后余数。原src、divisor、结果Reg、数值和控制保持，结果未改变时不生成材料。

JSON根reason与锚点kind均为integer_division_result_prune，字段为retained、input_results、output_results；角色拼写为quotient／remainder。原IntegerDivisionPair及其他生成材料保留在retained中，只说明此前发生的变换，不据此推定当前仍有两个结果或又执行过一次配对。多源单一trigger()／syntax()／origin()／instance()为空，来源从明确成员读取；未知成员仍为null。材料不保存旧Inst、输入值／Reg、常量、物理位置或另一结果表，SDK／JSON借用同一保留节点。

商余结果全部不再需要时，原操作照常删除，来源沿原EliminatedOperation::ValueResults进入同一ValueCode／Stage.generated_origin的UnusedOperations／EliminatedOperations；整个空Map随后沿EmptyMapRemoval交给Program。EliminatedOperation::integer_division_results()返回可选的原IntegerDivisionOp切片，JSON同名字段为quotient／remainder角色数组；Both有两个成员，单结果只有实际角色。result_types与该角色列表都按删除前原Inst::results／outputs顺序解释逻辑分量。普通值删除的该字段为null，来源未知与角色种类分别表达，不伪造定义或增加另一删除材料链。实际查询沿原bc ir的--module、--program及--entry选择原根，SDK直接借用原Inst、ValueCode、Stage或Program，不增加专用命令。

上述来源仅compiler，沿原IR／模块／SDK快照在最后实际消费者后释放；必要执行编码、资源ABI、借用、调度和完成不变，不为材料保活结果。输入／输出角色是本次变换的静态事实，不是运行次数、设备指令数或性能收益；已有范围继续开放，不按本批输入限定整数宽度或合法表达。新增公开枚举分支属于compiler SDK源码接口演化，按原软件版本规则处理。

#### 未消费直接私有槽写的删除来源

同一B消除算法确认StoreLocal(slot,value)的Local(slot)不在同一needed集合时，沿原删除点交接EliminatedOperation::DirectPrivateWrite { scalar, origin }。scalar直接来自当时registers[value]，表示这次写实际输入的Scalar；origin保留同一写的source_site与generated_origin。原写仍照常删除，RHS的Call／Print／Atomic等可观察求值继续沿原效果规则保留；不将本项扩大为外部存储或索引范围写删除。

该变体与ValueResults沿本次同一根输出Vec及GenerationReason::UnusedOperations／GenerationAnchor::EliminatedOperations { retained, operations }交给原ValueCode／Stage.generated_origin；空Map仍由原Program接收。JSON成员kind为eliminated_private_write，target为direct，字段scalar及origin；原有值结果成员kind继续为eliminated_value。SDK直接匹配公开枚举变体，result_types()对直接写为None，integer_division_results()也为None，不假造值结果或另一删除列表。

一条直接槽写是一个实际Scalar逻辑分量，其字节宽度由Scalar决定，不固定为四字节。原Local／Reg只服务当次IR，压缩重映射后可能不存在；材料不保存这些临时编号，不据此声称仍有物理位置、原存储身份、运行地址／值、执行次数或性能收益。静态来源不等同于运行采集，未知来源仍为null。此类型仅compiler，最后真实IR消费者后释放，不保留旧Inst／IR、值、声明布局、槽映射、活跃性表或运行资源，不改变原许可、数值和完成。

现有bc ir --module／--entry／--program查询与只读SDK消费同一事实；例如bootstrap/archive.b的layout、bootstrap/bindings.b各普通值函数以及applications/datasets.b应用入口。公共类型、枚举分支和JSON根字段的调整属于compiler接口演化，按原软件major规则处理；执行ABI独立。索引私有范围写由IndexedPrivateWrite明确区分，字段及语义见同名节，不能从直接写材料推定索引范围。直接写沿同一通用接收支持ValueCode、保留Stage及空Map后的Program；具体已验证范围与未知事实沿各自原台账记录。

#### 索引私有写删除的来源

同一B消除算法确认StoreLocalIndexed的base..base+len没有needed Local时，在原删除点交接EliminatedOperation::IndexedPrivateWrite { component_count, index_type, byte_offset, scalar, origin }。component_count是原len的逻辑分量数，index_type是原index寄存器的Scalar，byte_offset是原word的静态字节偏移，scalar是原value寄存器的写入Scalar；origin持有同一写的原source_site／generated_origin。index项沿原IR表达字节偏移，不自动当作源数组元素下标；其实际值与运行地址没有采集，不能由类型或偏移推算为已取得。

该变体沿本次同一根输出Vec及UnusedOperations／EliminatedOperations根交给ValueCode／Stage；空Map继续由Program接收。JSON成员kind为eliminated_private_write，target为indexed，字段为component_count、index_type、byte_offset、scalar、origin。直接写成员仍以target=direct区分，值删除仍为eliminated_value；SDK匹配同一公开枚举，result_types()及integer_division_results()对本变体为None，origin()借用原材料，不增加另一删除列表。

逻辑范围、实际写宽度与字段字节位置分开。例如array<u32,4>可对应component_count=4、byte_offset=0、scalar=U32；含u32 tag和u64 amount的Pair按现行布局组成array<Pair,3>时，写amount可对应component_count=6、byte_offset=8、scalar=U64，一次写八字节不等于六个数组元素或六个四字节字。材料记录删除时原局部范围的长度，不保留临时起点base／Local／Reg、不与压缩后的同号槽重新绑定，不能将已删除范围冒充当前存储身份。原声明布局及运行地址观察仍由各自原所有者负责，不复制旧IR、值、布局、槽表或活跃性图。

仅发生删除时记录，原needed判断、RHS可观察求值、数值、控制、访问及完成不变；有实际读取而保留的写不产生本删除事件。材料仅compiler，随原IR／模块／SDK快照及最后真实消费者释放，不保活指令、私有存储、阶段或运行资源，不进入必要执行编码；没有材料不证明未发生，未知原来源仍为null。现有bc ir --module／--program／--entry与SDK查询消费同一真实根，公共枚举分支按原软件major规则演化。

#### 归约统一纯值提取的来源

原CustomReduction::kernel按本次project／identity选择形成原ReductionKernel，prepare_reduction沿identity／project／combine的原顺序提取顶层统一纯值。实际移入原entry的Inst.generated_origin交接GenerationReason::ReductionUniformExtract及同名锚点；retained为提取前OperationOrigin，partition为ReductionPartition的Identity／Project／Combine，operation为ReductionUniformOperation，uniform_operand_count是同一次all_operands成功检查的实际逻辑Reg操作数访问次数。原直接source_site及既有生成来源分别保持，不保存旧Inst、Reg、输入值或另一uniform集合。

JSON reason及锚点kind为reduction_uniform_extract，字段为retained、partition、operation、uniform_operand_count；partition拼写identity／project／combine，operation拼写Constant、Uniform、BufferStride、Unary、Binary、Cast、Math、Select、ConstantIntegerDivide。类别来自本次提取器的实际纯操作匹配，不限制合法B程序或后续优化；ConstantIntegerDivide表示原IntegerDivide具有常量除数；全部所访问操作数已在原uniform集合，零Reg操作数类别可作为种子，访问次数不等于不同输入数；Uniform／BufferStride仍有原统一参数或布局来源，计数为零不表示没有运行输入，这些关系继续由原指令／接口字段表达。材料只描述当时实际消费的优化条件，不再运行一套证明或改变许可集合。

原extract不递归穿过Scope／If／Loop，不将非统一数据或资源载荷访问提到入口。未选的identity、未选／不存在的project没有本分区事件；原operation中的值函数仍可能保留，不能从存在推定本次参与。后续with_prelude沿原复用／合并来源保持已经发生的提取，分区角色不根据最终保留节点反推；多源单一trigger()／syntax()／origin()／instance()为空，已取得事实从明确成员读取，缺失来源仍为null。

SDK／ReductionKernel::ir_json查询同一本次实际内核，分区末尾、invariants与六角色向量继续由原字段交接，不为查询重新生成内核或调用编译／后端。提取后的入口位置与提取前的分区分别解释，逻辑Reg不等于物理寄存器；入口执行次数由原CPU／GPU调用与提交决定，不宣称整个集合全局只执行一次。材料只compiler，随原Inst／内核及最后真实IR消费者释放，不持有运行资源、不保活指令、不进入必要执行编码。公共枚举演化按原major软件版本规则，完整机器映射与运行测量另沿原职责。


#### 普通一元常量计算的来源

原unary_expr在operators::unary选出实际运算且输入Value.constant已知时，使用原constant::unary计算各逻辑分量。原constant_value产生的每个Constant保留原source_site，另持有GenerationReason::UnaryConstantFold及GenerationAnchor::UnaryConstantFold { expression, operation, scalar, input_bits, result_bits, component_index, component_count }；expression共享该Constant原语法点；当前一元SourceSite的字节范围是运算符token，不扩大为整个表达式，完整定义源码快照仍为原Source。材料不改变原常量、Proof、literal或分量顺序。

JSON的reason与锚点kind均为unary_constant_fold；operation按原UnaryOp拼写，scalar按原Scalar拼写；input_bits／result_bits是0x加十六位十六进制数字的字符串，Scalar决定实际有效位宽。component_index_base为0，component_index／component_count描述本次平坦逻辑分量，不能当作SSA寄存器或机器位置。原单源trigger()及锚点syntax()／origin()／instance()借用expression的已有事实；没有语法材料时仍返回空，不猜补来源。

实际运算包括FloatNegate、SignedNegate、UnsignedNegate、LogicalNot与BitNot，仍消费原位宽截断、环绕和浮点符号位规则，不重新解释源码或另作求值。单目加号与负号共用原数值／矩阵类型判定，随后只交回已验证原值，没有新运算事件。Unary(-,Number)由negative_number完成负字面量定型，特别是i32／i64最小值，不虚构同型正输入或普通SignedNegate；其原Constant及SourceSite、源码快照保留最终类型、位模式与原表达式。

后续复用／删除沿已有OperationOrigin及原ValueCode／Stage／Program根交接该事实，DCE不保活操作；运行期Unary不会得到常量折叠材料。材料仅compiler，随原IR及最后查询消费者释放，不留旧IR、AST／World或运行资源，不进入必要执行编码。模块const普通读取沿下节交接；阶段Literal的值IR接收沿后文交接，形状／变体编号仍各有来源缺口；不能把临时Builder中的材料当作最终能力。



#### 模块const普通读取的求值来源

原name_expr读取模块常量时，module_constant继续调用同一evaluate。临时Builder的原Constant指令若已有generated_origin，evaluate取得这些操作的OperationOrigin，在原类型／位模式结果中交接可选GenerationReason::ModuleConstantEvaluation与GenerationAnchor::ModuleConstantEvaluation { definition, constant, operations }。没有已有变换材料则不增加该锚点；不收集全部字面量、复制临时IR、保存Reg或再求值源码。直接负字面量定型与尚未交接材料的其他变换不会由这里猜补。

module_constant与具名条件输出的常量读取共用evaluated_constant，仍用原constant_value(...,None)生成实际结果分量及当前读取点的source_site，再让这些Constant共享同一个求值材料。operations是该次求值已取得的变换事实，不是逐结果分量的数据依赖：相邻操作若共享同一SourceSite与GeneratedOrigin，只交接一份相同材料；比较只用当次Arc引用，不把指针当作永久身份。嵌套常量仍沿已有材料继续交接；序列按本次原code顺序，出现数量不是唯一求值／运算／运行次数，不能据此推定每个操作影响每个结果分量。

JSON reason及锚点kind均为module_constant_evaluation，definition为真实原source_definition或null，constant为实际完整常量名，operations是已有operation_origin数组，成员保留原语法及generated_origin。anchor().origin()借用可用原定义，instance()返回实际常量全名；单源syntax()／trigger()为空，不能用常量定义冒充一条用户表达式。源码常量保存原不可变来源；已编译接口缺失提供者定义时仍为null，保留实际常量名和实际取得的运算事实，不把接口求值当作提供者实现源码或IR。

材料仅compiler，在原求值结果、新Constant及原CSE／DCE／空Map根间共享，最后IR消费者后释放，不进入必要对象／执行编码、不拥有运行资源或增加常量缓存／追踪表。原类型、数值、Proof、literal和执行保持。只验证合法性的求值没有结果IR，不伪造函数体；没有实际结果分量时不造Constant。具名B函数与外部C声明的直接条件输出，以及签名绑定的类型化OutputConstant及其物化，沿下两节交接；阶段Literal的值IR接收沿后文交接，形状／变体编号仍有各自缺口。本节不承诺保存所有未消费临时求值的完整编译历史。



#### 具名条件输出的模块常量物化来源

具名B函数及外部C声明的输出条件仍由原outputs::expression/path取得输入快照、returned与模块常量。常量名称分支经过原解析和公开性检查后，直接消费原evaluate返回的Ty／Bits及可选ModuleConstantEvaluation，再由与普通常量读取共用的Builder::evaluated_constant形成相同Constant与当前source_site。具名条件和普通name_expr消费同一已检查常量声明结果。

新事实沿已有module_constant_evaluation编码；它解释实际常量求值，不冒充条件自身全部运算、运行执行或逐结果因果图。原公开条件引用常量的可见性、数值／Proof、条件输出许可与初始化判断保持。真实源码与已编译接口的未知范围沿原定义合同，不从当前读取点猜补提供者来源；后续CSE／DCE及空Map根沿同一OperationOrigin持有材料，不保活操作或建立第二合同状态。

此路径仅compiler，最后IR消费者后释放，不进入必要对象／执行编码或拥有运行资源。函数指针签名先绑定为OutputConstant，其独立交接见下节；不能用直接Name分支替代该结果所有者。

#### 绑定输出契约的类型化常量来源

OutputConstant继续用ty与bits表示原合同数值；在compiler配置另有可选generated_origin，类型为Option<Arc<GeneratedOrigin>>。OutputConstant::new(ty,bits)只建立数值，来源为空。原bind_constants消费同一constant::evaluate的类型、位模式及可选ModuleConstantEvaluation，随原OutputCondition::Constant与实际cfn签名持有，不重求值或另建常量表。

字段投影、数组length、变体选择以及原数值规范化中的转换保留输入常量的共享材料；已有Constant经过identity时直接克隆。材料仍表示输入常量的实际求值，不单独声称保存投影／转换本身或逐分量的完整因果图。普通字面值没有已取得材料时保持为空，不能按类型或位模式查找、猜补来源。

OutputConstant的PartialEq／Eq、PartialOrd／Ord及原合同去重仍只消费ty与bits；来源不同不产生新函数签名或改变数值／Proof身份。必要对象编码仍只写原ty与bits，解码恢复同一数值并使generated_origin为空；runtime-only配置没有该字段。对象／缓存材料不因本字段增加版本或调试数据，不能把未编码来源写成跨对象保留能力。

实际输出条件的convert消费已绑定Constant时，共用Builder::evaluated_constant，把同一材料交给原Constant指令；直接source_site保持实际条件位置。已有ContractCondition或原复用／删除来源通过retained继续持有它，不为观察保活操作，也不增加运行指令。材料沿原签名／类型化结果及IR所有者，在最后编译／查询消费者结束后释放，不保留AST／World、临时IR或运行资源。



#### 二元常量折叠的来源

原Builder::binary先按operators及原类型转换取得有效Value，再按原广播规则调用constant::binary或整数constant::divide。只有两侧Value.constant存在且这条原计算实际成功，才在同一次结果物化的真实Constant上附加GenerationReason::BinaryConstantFold；比较命题known_value、动态运算和仍存在的IntegerDivide保持原职责，不因也得到Constant就冒充两侧常量折叠。原literal、Proof、Scalar／Bits、寄存器、直接source_site、除数合法性及浮点／溢出语义保持。

GenerationAnchor::BinaryConstantFold与JSON kind binary_constant_fold提供：expression（借用该Constant原SourceSite，可为null）、operation（该次binary实际接收的运算拼写）、scalar（原转换后的共同输入标量）、result_scalar（实际输出标量）、left_bits／right_bits／result_bits（十六进制原始位模式字符串）、left_component_index／left_component_count、right_component_index／right_component_count、component_index／component_count，以及retained（原结果已有GeneratedOrigin或null）。component_index_base为0；输入下标记录实际标量广播或逐分量选择，输出下标描述本次平坦逻辑分量，不是寄存器身份。比较输出为Bool，整数/或%按有效Scalar解释商余、符号及位宽，不增加另一运算编码。expression只是该次原计算点已有语法触发，运算可能来自原实现分解；不据范围反推操作、不扩大源码范围，也不为已有生成步骤补造直接源语法。

单源trigger()、syntax()／origin()／instance()只解释expression，原生成职责从retained读取；材料仅compiler，随原Inst、CSE／DCE／空Map根、模块及查询快照到最后IR消费者。模块／阶段常量的实际结果及其原接收者共用同一事实，不重新求值或保留旧IR、AST／World，不增加状态表、必要编码、运行指令或保活。材料描述这次实际常量计算，不声称完整输入谱系、运行次数、最终ISA或实测收益。形状／编号整数结果和Proof比较来源仍沿各自独立编号推进。

#### 数值转换常量折叠的来源

原Builder::cast沿constant::cast取得逐分量结果，保持原类型、宽度、符号、截断和浮点许可。只有原成功路径物化Constant，且该分量source Scalar与target Scalar不同，才在该原Constant附加GenerationReason::CastConstantFold；同类型复用不产生转换，混合记录中Scalar已经相同的物化分量也不产生转换。位模式相同可以有真实类型转换，不能按bits是否变化判定；未能完全常量求值时保留原Cast，bitcast沿原独立职责。

GenerationAnchor::CastConstantFold及JSON kind cast_constant_fold分别提供expression（原Constant的SourceSite或null）、source_scalar、target_scalar、input_bits／result_bits（十六进制字符串）、component_index／component_count。component_index_base为0，index是原结果平坦逻辑分量下标，count是整个结果分量数；它们不是寄存器身份、转换次数或存储字数量。源类型按该分量实际Scalar读取，混合记录不能以首字段类型解释所有分量。单源trigger()与syntax()／origin()／instance()只借用原expression，不补造／扩大范围。

原literal、Proof、source_site、Scalar／Bits、寄存器、布局与运行期转换语义保持。材料只归compiler的原Inst、CSE／DCE／空Map根、模块／阶段常量实际结果及查询快照，到最后IR消费者后释放；不保留旧IR、AST／World、完整输入谱系或运行资源，不增加求值器、来源表、状态、必要编码、运行指令或保活。静态转换记录不等于运行次数、最终ISA或实测收益。

#### 位重解释常量折叠的来源

原Builder::bitcast只在已检查输入持有完整Value.constant时，以同一位模式物化目标Scalar的Constant。各结果附加GenerationReason::BitcastConstantFold；GenerationAnchor::BitcastConstantFold及JSON kind bitcast_constant_fold交接expression（原Constant的SourceSite或null）、source_scalar／target_scalar、bits（十六进制字符串）及component_index／component_count，component_index_base为0。bits是源与目标共同的原始位模式，不是宿主浮点读数；分量是平坦逻辑位置与总数，不是寄存器身份或运行次数。源／目标方向来自原实际bitcast，不按bits相等把它误写成恒等数值转换。

动态输入仍发射原UnaryOp::Bitcast；原位宽、向量许可、数量／类型诊断、literal、Proof、source_site与布局保持。单源查询只借用已有expression，不补造未知来源。材料仅compiler，随原Inst、CSE／DCE／空Map根、模块／阶段实际结果与查询快照到最后IR消费者；不保留旧IR、AST／World、完整输入谱系或运行资源，不增加求值、状态表、必要编码、运行指令或保活。数值转换与最终机器映射各沿原职责。

#### 整数min／max已知结果的来源

整数min／max由原numeric_operands确定实际共同Scalar与广播关系，再由原bounds::arithmetic及整数已知值求值决定是否物化完整Constant。整数求值在同一次返回结果时交接IntegerKnownValueBasis：SingletonRange对应有效结果区间单值，ZeroCondition对应原非零条件真分支不可能且假分支可能；普通known_value共用该原求值，不另算一次。只有原成功物化接点产生GenerationReason::MinmaxKnownValue，浮点或未取得完整已知结果的原比较／Select不误标。

GenerationAnchor::MinmaxKnownValue及JSON kind minmax_known_value提供expression（原Constant的SourceSite或null）、operation、scalar、left_ranges／right_ranges、left_constant_present／right_constant_present、same_value、result_bits及basis（singleton_range／zero_condition）。区间为包含端点的原始标量位模式，按Scalar实际数值次序排列，不使用内部符号偏置编码。constant_present只报告整个原Value.constant存在与否，不冒充逐元素输入谱系。两侧component_index／component_count及结果component_index／component_count描述真实广播，component_index_base为0。

这些材料只解释本次实际结果和有效局部前提，不保留Proof／条件图或完整输入谱系。单源查询只借用已有expression，未知为null；原literal、Proof、类型／精度、source_site、布局及运行语义保持。材料沿原compiler Inst、CSE／DCE／空Map根、模块／阶段实际结果及查询快照到最后IR消费者，不保留旧IR、AST／World或运行资源，不增加求值器、状态表、必要执行编码、运行指令或保活。

#### 比较命题已知值的来源

原binary在常量运算未取得完整结果且comparison成立时，才查询原Proof的Bool结果；普通known_value与本接点共用原Bool分量求值，同一次返回结果与false／true分支可达性。只有该原路径成功物化整份Constant才附加GenerationReason::ComparisonKnownValue，不能仅按操作数constant是否存在判断，也不误标原BinaryConstantFold或仍需执行的比较。

GenerationAnchor::ComparisonKnownValue及JSON kind comparison_known_value提供expression（原Constant的SourceSite或null）、operation、scalar、result_bits、false_possible／true_possible、left_ranges／right_ranges、left_constant_present／right_constant_present、same_value、两侧与结果的component_index／component_count，以及retained原生成材料。result_bits为原Bool位模式；可达性来自同次原条件查询。component_index_base为0，constant_present只报告整个原Value.constant是否存在。区间端点包含且以原始Scalar位模式表示，interval_order为整数scalar_numeric、浮点float_bit_order，Bool区间及其次序为null。浮点位序不是NaN数值次序，不能用其端点重写原谓词或把!(a<b)改成a>=b。

这些材料是本次局部前提快照，不是完整输入谱系或证明图；单源查询只解释原expression，原生成职责从retained读取。写入／生命周期变化后的原Proof范围、literal、类型、转换、source_site、布局与运行语义保持。材料沿原compiler Inst、CSE／DCE／空Map根、模块／阶段实际结果与查询快照到最后IR消费者，不保留旧IR、AST／World、Proof／Clause图或运行资源，不增加重复求解、状态表、必要编码、运行指令或保活。

#### 阶段常量结果到实际值IR的来源

原Root::Literal只在编译器的阶段展开中存在，同一结果持有Ty、Bits与可选generated_origin，不增加运行参数或对象编码。stage_root读取具名模块const时，接收同一已检查常量声明的材料；env中已有Root仍克隆同一原值和材料，实际Literal的Name读取再由下述StageConstantReference交接本次绑定。直接Number／StaticU32及计数器初始值不伪造读取或折叠；无折叠Name的retained可以为空，本次真实reference仍保留。

stage_expression复用原Builder、表达式和合法常量检查。在结束临时Builder前，取得该次表达式新增Constant上已有的generated_origin，以及结果实际引用的入口Constant材料；无关env预装载、uniform或其他局部结果不进入本次来源。与模块const共用原OperationOrigin收集及相邻共享事实去重，仅按该次SourceSite／GeneratedOrigin的Arc引用比较，不建立Reg／来源表。没有材料不创建锚点；有材料形成GenerationReason::StageConstantEvaluation和GenerationAnchor::StageConstantEvaluation { expression, operations }，再与原Ty／Bits共同交给Root。operations可包含结果已经持有的真实绑定材料，不将普通引用解释为算术折叠；临时Builder同时接收原展开的domain_path，模块const自身的定义上下文保持。

JSON reason和锚点kind为stage_constant_evaluation；expression是实际临时求值上下文的source_syntax或null，operations是该次已取得的operation_origin数组。范围沿原Expr，当前可为一元运算符、名称或构造名token，不扩大成完整表达式；原不可变定义快照仍可读取。anchor().syntax()和trigger()借用已有expression，origin()／instance()由同一语法点取得；没有语法时保持null，不能按最终阶段名补造定义、实例或域路径。材料不是每个结果分量的完整因果图、全部输入谱系或所有临时求值历史，尚未产生来源的转换等变换仍沿各自原编号建设。

Builder::roots共用evaluated_constant，把同一材料交给实际Constant及已有source_site，原Value数值、Proof、literal、初始化、位置、布局和阶段选择保持。原CSE／DCE及空Map移除根按原OperationOrigin继续持有，不为查询保活操作或阶段；材料由原Root及实际IR／Stage／Program拥有，到最后编译／查询消费者后释放，不保留临时Inst、AST／World或运行资源。

本节完成具名const／临时表达式结果及值IR接收三个独立接点。整域调用的DomainCallLiteral现按原调用生成职责同时接收已有求值材料；阶段if的实际选择及阶段for的范围／迭代结果已按后文交给原Program及整域入口，不能以Root已持有材料代替这些实际消费者。具名阶段常量整值Name读取通过下述StageConstantReference交接原语法及已有材料，无折叠也有真实绑定；不因此制造算术折叠或承诺完整子表达式输入谱系。阶段选择不等于运行分支执行，编译期展开不等于运行次数。

#### 具名声明、调用及条件IR的来源

compiler的SourceDeclaration表示真实声明快照，SourceOrigin继续表示原实现定义。source()／span()／module()／declaration()分别借用原Source、原函数span、模块及声明名；representation()返回DeclarationRepresentation::Source或CompiledInterface，明确该声明从原源码或已编译接口取得。只有Definition的真实Source／CompiledInterface且无body时生产材料，Generated上下文不猜补。声明名与DLL／符号／实际签名分别表达，不能把声明当作提供者实现或真实签名／代码寿命的证明。

SourceDeclaration::ir_json为kind=source_declaration，module／declaration及representation=source或compiled_interface；其余source_path、source_sha256、source_bytes、UTF-8字节半开范围、行列及原始路径编码与SourceOrigin共用同一序列化职责。原Function.span原样交接，不扩大成完整声明或实现；完整Source快照从source()借用，查询不重读已改写／移走的文件。实现定义的原source_definition字段及查询保持。

实际具名C调用及c_function取得的静态函数地址使用GenerationReason::DeclarationReference及GenerationAnchor::DeclarationReference { declaration, retained }。原Call／FunctionAddress的source_site仍是实际调用者，retained是该指令原OperationOrigin，已有生成材料不丢失。JSON reason／锚点kind均为declaration_reference，字段为source_declaration形式的declaration和原operation_origin形式的retained。真实B实现没有这份声明材料时仍由原定义来源解释，不伪造成声明；运行期cfn间接目标不从签名或地址猜补命名声明。

outputs::expression只给本次convert／expr真实新发射的Inst附加GenerationReason::ContractCondition及GenerationAnchor::ContractCondition { declaration, span, retained }。JSON reason／锚点kind均为contract_condition，含declaration、retained及原条款span的同单位范围／行列，范围只用相应声明Source解释。真实常量求值沿retained保留；这是派生关系，不伪造成原用户表达式。若条件只复用bool输入／返回值而没有新Inst，不造假节点，实际Call的声明引用仍保存原合同来源。

上述多角色锚点的单源trigger()／syntax()／origin()／instance()均为空，各声明从anchor()的明确成员读取；空单源查询不代表没有来源。材料仅compiler，沿原Inst及已有CSE／DCE／根保存到最后IR消费者，不保活操作、旧IR、AST／World或运行资源，不增加源码登记、条件图或状态表，不进入必要对象／执行编码。调用、非零、访问、初始化、签名、值及资源完成仍消费原合同。公开枚举新增变体按现行版本规则处理，源ABI和资源ABI分别记录。


#### 空Map阶段移除的来源

原optimize末尾仅沿原Operation::Map且code为空的条件移除Stage。实际移除前，将同一Stage.operation_source及generated_origin作为OperationOrigin共享给原Program.generated_origin，使用GenerationReason::EmptyMapRemoval及GenerationAnchor::EmptyMapRemoval { retained, stages }；retained保留可选旧Program根，source_origin独立保持。Program::generated_origin()、anchor()及现有bc ir程序／应用资源查询消费同一真实结果，不新增另一程序或查询命令。

JSON根reason及锚点kind均为empty_map_removal，锚点有retained和stages；每项是原operation_origin，其中source_site对应被移除Map的原语句，generated_origin保留该阶段此前已形成的删除等材料。列表按本次真实移除顺序，不保存旧Stage、指令、Reg、阶段序号、运行地址或另一映射表，不是运行时间线。source_stages保持原优化前展开数量，stages列表只含实际保留阶段；全部阶段被移除时，Program仍可持有真实删除材料。代码为空、运行时数据集为空及空输入下仍须产出单位元的集合分别理解；原语义、阶段边界、数值、调度和完成不改变，不为观察保活空阶段。

多源单一trigger()／syntax()／origin()／instance()为空，各接点从明确成员读取，未知来源字段保持null。新一轮没有移除不制造事件，旧根及Source快照不可变共享；查询不重新编译、不读取后续改写的源文件，不持有AST／World或运行资源。材料随原Program／模块／SDK快照及最后IR消费者释放，compiler以外没有该字段／类型／getter，必要执行编码及资源ABI保持。新增公开枚举分支按原compiler SDK版本规则演化。
#### 降层归约内核的角色与分区查询

compiler SDK的CustomReduction::kernel(stage,project,identity)按本次选择产生原ReductionKernel；ReductionKernel::ir_json()只序列化这个实际结果，kind为reduction_kernel_ir。stage为原Stage::ir_json()，invariant_end、initialize_end和project_end为stage.code中的零基排他指令下标；依次划分公共入口、单位元、投影和合并四段，空段允许相邻末尾相等。invariants直接列出原prepare_reduction提取的逻辑Reg，不推定执行次数或最终机器存储。

source、projected、initial、left、right和result直接消费同一内核的六角色列表，元素引用本次stage.registers。它们已经过原值流binding；projected／initial／result是相应输出，不是另一份外部输入表。stage.operation仍含原源算法及project／identity／combine ValueCode，stage.code则是本次降层的代码，两者阶段不同。已选project的实际输入与source逐分量对应；未选project时source沿right，combine输入按left后right分量对应。原InputReadForward.input_index继续引用原ValueCode.inputs，来源沿该ValueCode.generated_origin取得；不把原函数Reg直接当内核Reg。调用者保留本次project／identity选择，operation中出现某个原函数不证明它参与本次执行，关闭identity时initial为空，关闭project时不补造投影调用。

查询不重新编译、优化、发射或运行，不重读源文件、不新增保存的Reg映射、拥有表或运行材料；生命周期沿原内核及共享来源到最后实际IR消费者。二次传播新读取事件由降层后的Stage另行交接，原值函数材料保持原阶段，统一提取变换原因也不由invariants列表代替。

#### 归约二次值传播的真实输入读取来源

原with_prelude在DCE后的值函数体再次传播局部值时，可能新增指向真实内核输入的读取替代。本次kernel.stage.generated_origin采用GenerationReason::LocalReadForward及GenerationAnchor::ReductionInputForward { retained, inputs }，retained保留可选旧Stage根；原stage.operation中ValueCode的代码与来源不改，不将其原阶段仍存在的读取描述成已经删除。

compiler SDK的ReductionInputReadForward::role()返回ReductionInputRole::Source／Left／Right，component_index()返回同一ReductionKernel.source／left／right列表中的零基逻辑分量序号，reads()借用原OperationOrigin列表，ir_json()输出kind=reduction_input_read_forward、role（source／left／right）、component_index及reads。锚点JSON为kind=reduction_input_forward、component_index_base=0、retained和inputs；根GeneratedOrigin.reason为local_read_forward。u64仍是一项逻辑分量，不拆成物理字编号。原合并输入先沿left／right消费；未选project时source与right相同，不重复来源或标成投影读取。无新事件不创建根，initial／projected／result不变成伪造外部输入。

新来源在原Inst结果交接后由同一次值流剩余事实直接形成，不重新优化、求值或扫描源码，也不新增保存的Reg映射、输入拥有表、IR副本或运行材料。单源trigger()及锚点syntax()／origin()／instance()为空，各实际读取从明确成员取得；编译器生成读取没有SourceSite时原样为空，不能删除该真实事件或补造语法。根与原来源共享不可变快照，随原Kernel／Stage在最后实际IR消费者后释放，普通原Inst／ValueCode及Frame前缀来源保持各自合同。

#### 局部索引写的现有IR事实与单位

Inst通过公开kind字段及来源查询交接原指令；所属ValueCode::ir_json()、Stage::ir_json()、FunctionBody::ir_json()及模块／应用IR查询编码同一StoreLocalIndexed和布局，指令JSON编码由内部辅助完成。字段含义如下：

| 字段 | 真实含义 |
|---|---|
| base | 底层局部存储起点的逻辑分量编号，不是地址或字节偏移。 |
| len | 同一底层局部存储的逻辑分量数，可覆盖包含该数组的完整声明；不是一次写入字节数。 |
| index | 寄存器编号，该寄存器提供动态字节偏移；字段本身不是已观察的运行时数值。 |
| word | 附加的静态字节偏移，不能仅按名称理解成四字节索引。 |
| value | 被写寄存器编号，其Scalar类型决定实际写宽度。 |
| locals | end_component及原Ty布局给出真实逻辑声明和物理字节位置；填充不形成逻辑槽。 |
| source_site | 原写已取得的语法位置，可能只指向目标表达式，缺失为空，不补造成完整赋值语句。 |

LoadLocalIndexed共享这些地址含义，dst为读取目标寄存器，其Scalar决定读取宽度。Native及SPIR-V按原声明位置、动态index字节偏移和word选择实际标量地址；逻辑存储范围、可能别名范围和实际写宽度分别表达。例如u64字段的word可为8，u64本身仍只占一个逻辑分量。静态IR不证明该操作已执行，也不交付未采集的实际地址或内存值。

当前原值流保守地在动态写点停止沿用相应底层范围内的先前局部值关系；该临时分析处理不等于写入全部分量，不等于初始化失效，不成为永久优化许可或语言限制。不同分析算法可演进，工具不要求把每轮缓存增删保存到来源链。确实消失、合并或替换的程序操作仍按实际生产者保留来源；临时状态的按需观察沿原所有者和真实存活期，未实现观察能力继续按原台账推进。

#### 指针与原子访问的既有别名事实

IndexedSource::Pointer的aliases与address、index、stride由原IR直接持有，现有指令JSON以kind=pointer及同名字段交接。MemoryAlias::Local { base, len }编码为kind=local、base、len，表示可能受影响局部存储的逻辑分量范围；MemoryAlias::Data(binding)编码为kind=data、binding，表示原数据绑定。别名不是实际地址、执行次数或一次写入的字节范围，address／index字段中的Reg也不是已采集的运行数值；实际布局沿原locals与Ty查询。

普通函数的原地址合同收敛后，check/address.rs::apply_memory将同一分析结果交回原Pointer.aliases，并递归处理原控制结构。未知外部影响按原分析保守覆盖局部声明，不据此推定所有局部已写；查询者须区分已交接的编译结果与构造中的中间IR，不能拿早期空列表作为无别名证明。这里说明当前生产／消费接点，不固定临时分析算法或建立另一份别名权威。

StoreIndexed直接交接source、offset、word、value。Atomic直接交接target、scalar、offset、word、operation、order及可选result；CompareExchange另在operation中保留expected、desired和failure内存序。AtomicOp::Load不写，Store不读，其余读改写可能写，比较交换是否成功须由实际运行判断。原值流按这些访问语义消费别名：原子Load不会触发局部写失效，数据读取缓存另行清除也不能解释成局部写。已有字段和所属ValueCode／Stage／FunctionBody查询足以表达本叶静态访问事实，不增加逐轮缓存维护来源；原IR、共享来源及返回查询值各按真实消费者释放。实际运行地址／值和可编程观察仍属后续运行工具能力，未实现不冒称已交付。

#### 原生调用的既有局部访问事实

Call.arguments中的Reference直接保留ty、access及address；CallReference.storage为Local { base, len }或Indexed(IndexedSource)，另有可选offset寄存器和word字节偏移。Local按本次IR给出的底层逻辑分量范围理解，被借字段不一定占满该范围；实际投影由原偏移及ty布局解释，分析与表示可以演进。现有JSON以kind=reference、type、access、address交接实参，地址继续编码storage、offset、word；来源沿原origin查询，未知不补造。Read、Write、InOut、Atomic的访问含义沿原Access，不把参数权限或静态布局当作一次实际内存写入。

Call.memory独立交接各MemoryAccess的alias与access，JSON为memory数组内的kind=memory_access、alias、access。不能只看Reference实参而漏掉按值地址背后的影响；普通函数合同闭合过程中，原apply_memory沿同一memory_use重填同一Call.memory；失败清理等实际闭合消费者结束后，由原模块最后交接撤去该编号。memory_use仅是原构造／分析阶段的关联编号，完成交接后可为空，不作为稳定调用身份或另一份运行状态。查询须以本次实际阶段为准，未交接的中间空结果不证明无影响。

内部Call::locals(true)收集两类可写Local范围：Reference的真实访问与地址别名，以及memory中的真实可写效果。值流据此停止沿用旧局部值；Read效果本身不撤销局部知识，Data绑定另按原数据存储规则处理。保守范围不证明运行覆盖全部分量、初始化失效或外部代码可信。当前SDK公开字段及所属ValueCode／Stage／FunctionBody的JSON已提供这些原事实，不保存逐轮locals.remove历史；原IR及查询结果按最后实际消费者释放，不接管代码或运行资源。调用合同生产的独立缺口、运行时地址／值、实际完成及B可编程观察入口仍分别负责。

#### 循环的既有控制与可能局部写事实

原InstKind::Loop { condition, test, body }及其所属IR查询直接交接循环结构，JSON为kind=Loop和同名字段。condition是代码序列，test是逻辑寄存器编号，body是循环体序列；不能把condition读成一个运行时bool，也不能把test编号当作已采集的判断结果。每次检查先执行condition并使用test，选中时进入body，再按回边继续；循环体为零次仍不等于条件代码未求值或没有副作用。

Break离开最近的Loop，Continue转到该Loop的下一次检查；Exit只离开最近的Scope，不是通用的循环退出或整个并行实例终止。现有JSON以Break／Continue／Scope／Exit及实际嵌套结构表达，作用范围由原结构解释，不另建目标图。写事实沿同一原操作查询：StoreLocal给出逻辑槽，StoreLocalIndexed给出底层逻辑范围及字节寻址，StoreIndexed与可能写的Atomic给出原别名，Call同时给出引用访问和memory效果。类型／布局、可能范围、实际一次写宽度及运行命中分别理解；原子Load和只读调用效果不能当作局部写。

当前B值流优化对同一Loop的condition与body沿原visit／Inst::local_writes汇总可能写，并停止沿用对应入口旧值。该保守分析不证明循环必运行，也不固定为语言的永久优化范围；written及locals是原分析存活期内的临时状态，不保存为逐轮来源历史。SDK公开字段及原ValueCode／Stage／FunctionBody的JSON提供同一原IR，查询结果随实际消费者释放，不持有运行资源或复制控制权威。真实删除／替换来源、合流、按需分析现场观察、B可编程入口及运行时状态继续按各原任务交接，不由本节静态查询代替；最终IR也不冒称保存了此前全部内部分析过程。

#### 控制合流与实际读取替代

原IR的If直接给出condition、yes、no，Scope／Loop与Break／Continue／Exit保留真实嵌套，局部读写、调用效果及别名继续沿原字段查询。读取真实被替代时，原LocalReadForward／LocalInputForward／ReductionInputForward交接删除读取及实际保留结果／输入；仍保留的LoadLocal不能仅因没有替代锚点而被解释成读取无效。各Reg只在其原IR寄存器表及版本内有身份，不跨函数／阶段按相同编号推定同一定义，也不把两条路径的可能数值相等当作同一SSA。

当前内部Flow区分next、breaks、continues、exits四类出口，各类只合入仍到达该出口的路径。merge沿原别名root保留共同的同一局部值；None表示没有该类路径，Some空映射表示有路径而没有共同已知值。Scope将自身Exit接回继续点，Loop消费自身回边与break并保留外层Scope退出；退出分支不参与普通继续，单次迭代定义也不成为循环后值。这是当前分析语义说明，不导出另一份Flow对象、稳定槽值表或历史查询格式；现有SDK／JSON直接交接原控制和真实读写／替代材料，分析算法可以演进。

最终IR不保存此前每轮临时知识，也不代替真实删除／重排来源。原block在无继续路径后结束迭代，不等于已经删去后续IR；当前真实模块及正式CPU／GPU示例的观察中，该处没有剩余操作，不新增专门来源记录。原语句生成已经按其Flow停止，有限观察不成为未来合法IR的限制；真正发生的程序删改仍按原结构变换交接，完整来源能力未由本项调查代替。原IR和查询值由实际消费者持有至结束，不拥有代码或运行资源；按需分析现场、B可编程入口及运行状态仍按原工具范围推进。

#### 应用入口与根Frame的生成来源

原compile_application从本次真实选择的Instance构造Definition锚点，Application.body的generated_origin为application_entry；其原生Call以application_entry_call交接同一实际入口。它们属于原隐式包装，不额外创建FunctionBody或把根定义Span伪装成一次用户调用。普通源函数、集合值核及其他域包装的ValueCode.generated_origin不借用此入口，原source_origin／entry_origin保持各自语义。用完整应用IR查询观察body；函数选择仍返回实际B函数，不能把它当根包装。

live_resources仅在原Frame实际被保留的路径交接错误接收器、上下文、create、就绪检查／分支、失败退出值和正常destroy等真实原因。每个节点仍是原操作，FrameReadiness的多个节点共享同一不可变材料；各理由的Definition锚点共享原入口定义快照及实例。根锚点说明应用拥有的隐式生命周期，不声称某个资源表达式独自导致Frame，也不覆盖其余资源原语的实际源触发。没有活资源、或资源路径已经消除时，不生成Frame接收器、create、就绪分支或destroy；原入口包装仍有自己的生成材料。

Frame失败分支与正常路径保持原代码／原语义，静态生成材料不证明分支已执行。正常资源入口沿原CPU／GPU及Frame资源完成运行，根元数据不拥有或延长运行资源，不新增条件、Builder状态、调用图、拥有表、调度、释放或错误路径。Source、名字与材料随原IR不可变共享，最后IR消费者后释放；查询不重读后续磁盘文件，不持有World／AST，只有显式查询承担摘要、行列和JSON成本。完整优化、重写／消除及机器范围仍由原叶交接，不为材料保活代码。dataset分配沿下一节的真实构造触发交接；生成函数初始化另沿原factory引用交接，新绑定及重新赋值槽位、dataset最终存储释放及拥有元素逆序清理循环另沿原发射上下文交接；其余清理、扩容、寻址、参数接收、传输及计算发起的细分职责仍待实现，不从根锚点猜补。

#### dataset构造分配的生成来源

原create_application_dataset把本次真实构造表达式形成的Syntax锚点交给同一application_call；原allocate_<type> Call的generated_origin.reason为dataset_allocate，Rust理由为DatasetAllocate。trigger()与anchor().syntax()都借用该原SourceSite，保留真实定义快照、实际泛型实例、从外到内的inlined_at及domain_path。当前解析器构造Expr.span覆盖dataset标识符，JSON如实交接这七个UTF8字节，不扩大为未取得的类型、长度或factory完整参数范围。缺失原来源时trigger明确为null，生成职责仍可存在；不从内部符号名、根定义或已发射最后一条指令猜造来源。

allocate是由B构造生成的运行库包装Call，source_site为null；实际B触发位于generated_origin.trigger。长度表达式、可选factory的已有语法来源和真实普通函数体不因同属构造而改成分配步骤。原类型索引、长度参数、Frame参数、返回句柄与原Data保持；零长度仍沿原构造规则发射其真实allocate，不因观察删除分配，已消除分支与没有原分配的未初始化声明则不补造操作。application_declare只建立原未初始化编译模型，不发射原生初始化；新绑定与重新赋值槽位沿相应节分别交接；生成函数初始化沿下一节的原factory触发交接。

材料只由原Inst编译期持有并随最后IR消费者释放，不拥有Frame或延长实际存储寿命。静态Call不表示分配已运行或已经成功，运行失败仍沿原执行边界；没有增加运行时采集、条件、注册表、Builder状态或释放链。address的原请求常量及Call按下文Frame地址来源交接，不携带分配分类；storage_address按下文存储地址查询来源交接；upload／download仍保持原source_site，各自完整生成来源由原叶交付。优化重写／消除及机器范围继续沿原.7.3／.7.4推进，不为观察保活操作。

#### dataset生成函数初始化来源

原initialize_application_elements以实际factory Expr.span取得一次SourceSite；同次展开的六类GeneratedOrigin共享该不可变Syntax锚点。trigger()和anchor().syntax()都借用原引用触发，保留实际调用方定义、具体泛型实例、inlined_at及domain_path；它不是callee定义或用户显式调用。当前Name／FunctionRef的原范围覆盖引用标识符，泛型参数和整个dataset构造不补造到该跨度。原allocate继续使用自己的dataset构造锚点。实例选择、类型／签名和合法输入仍由原检查器完成，不引入运行期反射、通用函数对象或额外调用图。

DatasetInitializeCursor标明原准备零值与实际起始StoreLocal，Condition标明原LoadLocal及Less，Call标明真实factory Call，Write标明写入零offset及各原StoreIndexed，Advance标明原一常量／IAdd／StoreLocal，Loop标明原循环本身。只有实际发射的节点有其理由，种类及顺序保持；同一原常量生产接点完成截断、寄存器与Value构造，普通constant仍显式没有本项生成材料，不能根据寄存器或最后一条代码猜补。原StoreIndexed的每个真实分量、宽度与偏移保持；共用application_memory／地址包装的完整来源仍归.5.5，普通用户写入不借用初始化分类。

原invoke_returned沿既有分派交给native_function或external_call／c_call，生成材料在同一实际Call发射处交接，source_site为null，真实factory引用位于generated_origin.trigger。普通B或已编译目标保持原CallTarget，callee函数体／源码仍属于原函数；外部C目标保持实际DLL与符号，没有原B实现时不造函数体。普通用户调用、集合值核、静态释放及原生cfn调用显式不携带初始化材料，原source_site、C ABI、前置／输出合同、实参、返回、副作用和完成保持。生成材料不改变调用执行路径。

零长度类型检查保留原规则；原准备零常量可以先被发射，但原值优化可以删除它，返回IR中不因此留下游标或调用。动态长度在运行时为零仍可有静态Loop／Call／StoreIndexed，不能凭静态材料声称它们运行过。现有值优化还会合并重复零offset或一常量；保留下来的原节点继续携带材料，被删除或新构造节点的完整多来源及消除交接仍归.7.3。当前返回IR的缺失材料不是已交付消除记录，也不为观察保活常量、循环或资源。

原Data／Returned／Place／Frame继续初始化和拥有元素的唯一交接，父投影与完成／释放不变；来源只归编译期原Inst共享且最后IR消费者后释放，不持有World／AST或运行资源，不增加Builder状态、运行期拥有表、记录器、调度或释放链。查询不重读后续磁盘文件，只有显式查询承担摘要、行列和JSON成本。

#### dataset新绑定的局部槽位来源

原bind_returned把实际Dataset交给application_bind，只有原rebound选择的mutable分支需要局部槽位时，两个原StoreLocal共享DatasetBindSlots，JSON reason为dataset_bind_slots。它们按原顺序写入句柄、长度，仍使用同一原Reg／slot及Ty::owned_dataset布局；source_site为null，触发位于generated_origin的Syntax锚点。原ValueCode.locals交接真实局部类型／分量区间，StoreLocal.slot是IR局部标量槽号，不是最终栈地址或Frame物理存储地址；数据描述与实际资源存储分别解释。

材料直接共享当时原发射SourceSite，定义、实际泛型实例、inlined_at、domain_path、语法类别及字节跨度与原触发完全相同，不增加Span参数或重造一个绑定位置。当前Let与Match等上下文可覆盖原关键字；匹配分支的触发不能当作单独模式变量名或完整绑定语法。没有原SourceSite时trigger明确缺失，不从变量名、槽号或最后一条指令猜造身份。trigger()／anchor().syntax()及原只读查询继续借用同一原材料。

没有原mutable槽位写入时，不增加槽位、寄存器或生成材料；application_declare只建原未初始化编译模型。重新赋值的原槽位写入、其他类型的StoreLocal、RHS分配／初始化、原释放／移动分别保持，不借用新绑定理由。原Data、Place、Proof／initialize、rebound选择及Frame最后消费者不变；局部数据描述写入不是复制底层资源或取得另一份拥有。材料只归编译期原Inst共享，最后IR消费者后释放，不新增Builder状态、资源／绑定表、运行期采集、AST／World保留或释放链。

原值优化可以删除未使用、被覆盖或已经传播替代的槽位写入；返回IR只保留其实际存在的来源，不为观察保活写入、固定rebound策略或冻结优化。材料缺失不能直接断定原代码没有建立绑定，完整重写／消除记录和机器位置仍归.7.3／.7.4及.11。

#### dataset重新赋值的槽位写入来源

原application_assign先完成RHS求值、类型匹配、所有权交接及目标旧资源的必要释放，再将原赋值SourceSite共享为DatasetAssignSlots的Syntax锚点，JSON reason为dataset_assign_slots。原write_storage_value沿Ty分量投影顺序将同一可选材料交给原write_place；只有原Local分支的真实StoreLocal持有它，句柄后长度的顺序、Reg／slot、位置与Proof／initialize保持。source_site为null，真实语法触发位于generated_origin.trigger；原定义、具体实例、inlined_at、domain_path及当时实际字节范围不变。赋值语句通常保留原目标标识符范围，不扩大成完整赋值表达式；没有原SourceSite时如实缺失，不猜名字或末条指令。

未初始化声明后的首次赋值、移动后补回、自赋值、拥有形参及资源返回的赋值仍沿原语义；是否实际保留Local写入由原优化决定。application_declare本身不造初始化，新绑定另用DatasetBindSlots，RHS分配／初始化和旧资源释放不借用重新赋值理由。普通值、拥有聚合与元素替换没有该可选材料，其他Place保持原发射职责；不复制访问检查或建立另一份资源赋值实现。

这些槽位描述原局部数据表示，不是运行时栈地址、底层存储复制或另一份拥有。材料只归compiler功能下的原Inst共享并在最后IR消费者后结束，不增加Builder状态、拥有表、采集、隐式同步或释放链。原优化可消除被传播、覆盖或未使用的写入；静态来源不证明赋值已经实际执行，返回IR不冒充完整重写／消除历史或机器位置，完整能力仍归.7.3／.7.4及.11。

#### dataset最终存储释放的生成来源

原application_drop完成已有访问／所有权检查、取得实际句柄并清理拥有元素后，在最终application_release产生的同一原Call上附DatasetRelease，JSON reason为dataset_release。Frame与句柄参数、外部目标、寄存器和先后顺序保持；source_site为null，原发射SourceSite直接共享为generated_origin.trigger的Syntax锚点，原定义、实际实例、inlined_at、domain_path、字节范围与语法类别不变。

这是既有发射上下文，不等于完整清理原因或精确的右花括号位置。显式return、break、continue、覆盖及被忽略结果可能保留原关键字或调用标识符；隐式函数结束及合流等没有原SourceSite时，trigger明确为null。外围if／while的范围不能当作独立退出语法，不从函数名、槽号、句柄或邻近操作猜补来源。即使触发缺失，真实生成理由仍可说明这是最终dataset存储释放Call。

原移动、返回保留资源、覆盖旧资源、CPU／GPU位置、拥有元素逆序清理及前后Ownership状态不改。原Data／Frame继续负责唯一存储和最后真实消费者，分类不增加release，不把运行库Call当作已经完成的运行事件。普通B或外部库中同名release函数不按名字分类；FrameDestroy、分配／初始化、新绑定及重新赋值仍是不同生成职责。拥有元素逆序清理循环来源见下节；用户静态F调用、条件拥有及活动变体清理选择的完整生成来源仍归.2.4.5.3.3至.5，原地址访问包装归.5.5，不能以本节代表完整清理来源。

材料只由compiler功能下的原Inst不可变共享，并在最后IR消费者后结束，不持有运行资源、AST／World或新增Builder状态、拥有表、清理图、调度和释放链；关闭compiler时不进入运行期字段或必要执行编码。原优化仍按实际行为保留／改写IR，完整优化与机器映射另归原叶，不为来源保活代码。

#### dataset拥有元素逆序清理循环的生成来源

原drop_application_elements在已有零长度判断、Data刷新和CPU访问条件之后，将同次原发射SourceSite共享给原清理循环及必要元素清理调用。原长度写入游标槽是DatasetDropCursor；条件的LoadLocal、零常量与Greater是DatasetDropCondition；选前一个元素的常量一与ISub是DatasetDropIndex；读取实际元素标量分量的ViewLoad是DatasetDropElementRead；回边StoreLocal是DatasetDropAdvance；外层原Loop是DatasetDropLoop。Unknown／Input／Cleanup元素的一般清理Call是DatasetElementDrop；Known／Common／Selected直接沿局部拥有树清理，不生成该Call。各节点source_site为null，语法触发位于generated_origin.trigger，原定义、实际实例、inlined_at、domain_path及字节范围不变。没有原SourceSite时trigger仍为null，不把整个函数范围或邻近语句补成退出语法。

元素载入沿原read_storage_value的Ty标量投影、read_place的ApplicationElement检查以及read_application_place的唯一ViewLoad生产者交接可选element_read_origin。原访问／初始化／Proof、常量语义、地址／索引／stride／word、Reg与槽位保持；普通读取明确没有该材料，其他Place不借用此理由。原Frame地址包装、refresh取得描述的读取、静态F及最终release保持各自职责；没有复制读取检查或新增解释执行／旁路读取。常量复用既有constant_with_origin，不另写常量或数值规则。

原循环仍从实际length逆序处理元素。Known／Common／Selected在实际装载处绑定局部选择，由原drop_owned_tree结束仍拥有的子树，局部空位不进入普通完整拥有参数。Unknown／Input／Cleanup经原native_element_drop及Module.units形成普通类型化清理callee，按原cleanup合同执行实际F、逆序子资源结束及同一Frame释放；同型后代保留静态调用回边，不按未知数据深度继续内联。当前元素结束后再更新原游标。嵌套拥有元素产生自己的原循环及材料；静态F函数体和调用、条件拥有及活动变体选择不整体改标清理循环。原长度确定为零时不创建游标、循环或读取；原优化可合并重复零／一常量或删除未消费字段，返回IR只保留实际节点，不伪造所有声明分量都已经读取，不为观察保活。生成来源不是循环已执行、析构已调用或最后设备消费者已完成的运行证据。

Data／Frame仍是唯一实际资源所有者，原CPU／GPU位置、资源ABI及最后消费者保持；材料只归compiler原Inst不可变共享，最后IR消费者后结束，不新增Builder状态、拥有表、资源副本、调度或第二清理链。完整消除／多来源与机器位置仍沿.7.3／.7.4，用户静态F、条件拥有及活动变体来源见以下各节。

#### 用户拥有值静态析构调用的生成来源

原drop_user_owned装入实际载荷、完成原资源可移动／保留借用检查并选择静态F实例后，将UserOwnedDrop交给已有invoke_returned的generated_origin参数。普通B函数、真实extern C及已编译函数沿各自原生Call生产者消费同一材料；参数、调用目标、静态特化、返回及副作用保持，不增加转发器、登记或另一调用／释放链。实际生成Call的source_site为null，generated_origin.reason为user_owned_drop；trigger为当时原SourceSite形成的可选Syntax锚点，原定义、实例、inlined_at、domain_path及字节范围保持，缺失为null。

触发可能是return、覆盖位置、外围语法，也可能没有材料，不将其解释为F声明或精确作用域出口。F函数体保留实际源码来源，外部或已编译接口不伪造函数体；普通用户直接调用同一函数不按函数名标成析构。移动、into_inner及其他没有原析构Call的路径不补造节点，原优化继续按真实语义进行，完整消除和多来源仍归.7.3。

材料仅在compiler功能下由原Inst不可变共享，最后IR消费者后结束，关闭compiler没有这些字段或运行期登记。载荷、CPU／GPU资源及代码寿命仍沿原唯一所有者和最后真实消费者；不为观察延长变量／资源寿命，不修改必要ABI和执行编码。静态调用来源不证明析构已执行、外部API关闭成功或完整内存安全。

#### 条件拥有清理分支与条件准备的生成来源

原drop_returned的Dataset与OwnedValue条件拥有分支，在保持原Ownership、载荷及保留借用交接后，把当前原SourceSite形成的可选Syntax锚点共享给两种实际生成材料。owned_when通过显式可选branch_origin及condition_origin消费用途；没有本项材料的其他用途保留原普通发射。实际外层清理If取得ConditionalDropBranch，JSON reason为conditional_drop_branch；presence_register内真正发射的条件准备取得ConditionalDropCondition，reason为conditional_drop_condition。body里的用户静态F、dataset最终release和活动变体选择分别保持各自来源，不把它们改成条件计算。

Presence::Input直接使用原已有寄存器，不新增读取。All／None和静态Selected只沿原常量规则，动态Selected沿原下标及常量产生比较，Not沿原一元操作；Both／Either保留原短路If、布尔槽位写入与LoadLocal，不改为提前计算两边。短路准备中的If属于conditional_drop_condition，不能据此算作外层清理If。原固定分支直接构造的StoreLocal原本没有普通source_site；它仍没有用户操作来源，可独立携带真实清理触发。生成节点的source_site为null，触发位于generated_origin.trigger，原定义、实例、内联／域路径及字节范围保持；原触发缺失为null，不用函数范围或邻近语句猜出口。

condition_truth确定条件时沿原路径跳过或执行body，没有实际If或条件准备时不造节点。原path及initialized临时保存／恢复、实际寄存器／槽位、Proof、借用、CPU／GPU资源及最后真实消费者保持。材料仅compiler下由原Inst不可变共享，最后IR消费者后结束，不增加Builder状态、运行时拥有标记、第二状态权威或释放链，不进入必要ABI／执行编码，不为观察保活代码和变量。完整消除／多来源仍沿.7.3，静态材料不证明运行时已经释放、设备完成或完整内存安全。

#### 活动变体清理选择的生成来源

原drop_variant只在实际需要判断当前变体时，将当前原SourceSite形成的可选Syntax锚点共享给VariantDropBranch及VariantDropCondition。前者交给原实际清理选择If，JSON reason为variant_drop_branch；后者沿owned_variant_condition与variant_condition的已有判别规则，把可选材料交给原常量与数值操作生产者，reason为variant_drop_condition。普通match／try及其他调用传None，不按函数名或变体类型推断清理用途。

原判别分量通过result_word复用已有值，没有原读取操作时不新增读取。已知载荷存在性沿原布尔常量规则，动态判别沿声明编号常量及原binary比较；常量折叠复用constant_value及constant_with_origin，原数值类型、运算选择、Proof和已知值规则保持。原通用调用只适配显式无生成材料，其他数值程序不因此改标。最后一个载荷沿原unpack_variant直接交接，原已确定条件沿原分支执行，没有原If时不补造选择；原优化仍可复用常量或删除未消费材料，不保活节点。

原payload装载／解码、条件拥有、静态F和最终release各保留自己的来源，普通match及try判别不整体标成清理。生成节点的source_site为null，trigger为原触发或null，定义、实际实例、内联／域路径及字节范围保持，不将return／覆盖位置或缺失材料猜成变体声明或精确作用域出口。材料仅compiler原Inst不可变共享，最后IR消费者后结束；关闭compiler没有这些字段，不进入必要ABI或执行编码。原活动载荷、布局、参数、借用、静态F顺序及最后真实CPU／GPU消费者保持，不增加Builder状态、拥有表、变体登记、另一个判别算法或释放链。静态来源不是释放已执行、设备完成或完整内存安全证明；完整消除及机器映射仍沿原编号。

#### resize填充值暂存的生成来源

resize_application_dataset在原第三实参已经求值、检查并取得returned_storage后，只为原local_slots及实际StoreLocal交接DatasetResizeFill，JSON reason为dataset_resize_fill。各逻辑分量保留原槽位及寄存器，共享一份不可变GeneratedOrigin；source_site为null，可选Syntax触发由原第三实参Expr.span及当前定义／实际实例／内联与域路径构成。普通调用或构造的Span可能只覆盖原标识符，按取得的范围交接，不猜补完整参数列表；触发缺失为null，不用resize根定义替代填充值来源。

填充值自身的用户计算保持原语法来源；实际resize Call、dataset句柄／长度绑定及清理各保留自己的职责。原宽标量、向量、记录、拥有载荷及字段布局、求值次数和顺序、静态模型、借用与Frame的实际存储版本继续交接。零分量填充值没有StoreLocal，不为观察增加伪操作；零字节引用按原本地范围与x64调用帧处理。材料只由compiler功能下的原Inst及FunctionBody／模块持有，最后IR消费者后结束，不进入必要ABI／执行编码或保持运行资源。它不证明实际扩容、分配成功或GPU完成；扩容Call由下文独立交接来源，优化消除及机器关系沿原编号继续。

#### 原生resize调用的生成来源

resize_application_dataset与take_last_application_dataset沿resize_application_storage发射同一blink_resize调用；原错误与仅成功采用的输出分别交接，原新SSA身份保持；仅该实际Call取得DatasetResizeCall，JSON reason为dataset_resize_call。节点source_site为null，可选Syntax触发取原resize表达式Span及当前定义／实际实例，inlined_at与domain_path沿同一快照保持。当前Span是原resize标识符，不扩大成未取得的完整参数列表；没有源定义时保持无触发，不从填充值或根函数猜调用位置。普通用户函数即使名字包含resize也不据此改标。

原Frame／根句柄、请求长度及只读fill暂存引用继续交接，静态模型、返回及新存储版本没有额外指令时不伪造步骤。填充值计算及DatasetResizeFill、数据绑定和清理仍保留自己的来源；零宽值也只标记实际Call，不补造StoreLocal，静态未执行路径没有本项材料。生成材料只归compiler下的原Inst及FunctionBody／模块，最后IR消费者后释放，不进入必要ABI／执行编码，不增加Frame、存储或释放链，不证明运行已扩容成功或设备已完成。完整变换及机器映射继续独立交付。

#### Frame地址请求的生成来源

application_address在既有可用性、借用及CPU位置检查后，沿原constant_with_origin及Inst::generated，把原bytes常量及同一无错误结果的blink_address Call标为DatasetAddress，JSON reason为dataset_address。两者共享不可变GeneratedOrigin；source_site为空，可选Syntax触发直接借用原当前SourceSite，语法种类、实际范围、定义／特化实例、inlined_at和domain_path保持，不把错误诊断Span重解释成另一语法，也不用资源根定义猜位置。缺少当前语法时trigger为null，生成职责仍明确。

Frame、handle、offset、bytes、stride、count、双U32地址结果、读写权限、布局、检查及最后消费者按原合同保持。标记只属于该实际生产者，不按符号字符串给普通用户函数分类；storage_address、根地址表示、索引乘加、读／写区及投影准备仍归各自叶项，原ViewLoad／StoreIndexed与既有初始化／清理职责保持。后续优化照常合并／消除常量，没有保留下来的指令不补造材料，不因此保活变量或冻结优化。完整来源变换与机器指令映射仍按原编号推进。

材料仅在compiler角色的原Inst及FunctionBody／模块中持有，原SDK／CLI来源快照消费者结束后释放；不进入必要执行编码、不拥有运行地址或资源、不增加调用、Frame、状态表、采集或释放链。静态地址Call不证明已经运行、地址此刻有效或GPU已完成。

#### 存储地址查询的生成来源

application_reference_value在原资源句柄必须进入地址型借用ABI时，仍发射同一blink_storage_address Call；仅此真实生产者通过Inst::generated的原来源参数交接DatasetStorageAddress，JSON reason为dataset_storage_address。生成节点source_site为空，可选Syntax触发直接借用原当前SourceSite，保留实际语法种类、范围、定义／特化实例及inlined_at／domain_path；未知为null，不将诊断Span或资源根定义猜作调用位置。普通B同名函数不据名字分类。

原Frame／根输入、U64结果、错误、view借用Proof及资源最后消费者保持；已经是U64根时直接复用，不产生或虚构查询。原字段布局准备、stride拓宽和参数槽位打包各沿其原叶交付，Frame地址请求的DatasetAddress也保持独立职责；本项不增加取址、权限、布局或范围检查，不改原真实读写和运行生命周期。静态查询不证明调用已运行、地址此刻有效或设备已完成。

材料仅由compiler角色的原Inst及FunctionBody／模块持有，沿原SDK／CLI来源快照消费，在最后IR消费者后结束；不进入必要执行编码、不保活变量、不拥有内存，不新增Frame、地址表、运行采集、释放链或包装函数。完整优化来源与机器映射继续归原编号。

#### 根地址表示准备的生成来源

application_root_address为同一原根地址保留两条真实路径：U64根经既有移位常量、逻辑右移和两个Cast拆为双U32；句柄经原零偏移、步长及数量常量交给既有Frame地址请求。只有该生产者实际发射的Constant／Binary／Cast取得DatasetRootAddress，JSON reason为dataset_root_address，共享本次不可变GeneratedOrigin。source_site为空，可选Syntax触发直接借用原当前SourceSite，语法种类、范围、定义／特化实例及inlined_at／domain_path保持，未知为null，不把错误诊断Span或根定义猜作访问位置。

原CPU位置检查、根表示选择、64位位型、寄存器和返回地址保持；不新增寻址、转换、检查或运行调用。寄存器分配、模型和证明没有发射指令时不伪造节点。Frame地址Call及其bytes常量仍是DatasetAddress，storage_address仍是DatasetStorageAddress；后续索引乘加、字段布局、读写区及投影准备各归原叶，不能把整个地址流程统一改标根准备。常量折叠、去重及消除继续按原语义，消失的节点不补造，不为观察保活变量或冻结优化；完整变换来源及机器映射继续归原编号。

材料只由compiler角色的原Inst及FunctionBody／模块持有，沿原SDK／CLI来源快照消费，在最后IR消费者后结束，不进入必要执行编码。运行地址、权限、存储及最后消费者仍归原Frame和真实资源所有者；本项不取得内存所有权，不增加地址权威、运行状态表、采集、Frame或释放链。静态表示转换不是实际访问成功、地址此刻有效或设备已完成的证据。

#### 索引与步长寻址的生成来源

offset_application_address仍消费原双U32地址、index、stride及offset，经原拓宽、拼接、乘加和再拆分交回同一双U32有效地址；仅该实际生产者的Constant／Cast／Binary共享DatasetOffsetAddress，JSON reason为dataset_offset_address。source_site为空，可选Syntax触发直接借用原当前SourceSite，真实语法种类、范围、定义／特化实例及inlined_at／domain_path保持，未知为null；不从根定义或邻近行猜位置。

原拓宽闭包曾直接Inst::new而未保存source_site，这不表示实际Builder上下文未知；当前已知触发沿同一生产者交接，缺失仍明确缺失。原宽度、地址位型、算术顺序、寄存器与返回保持，index／stride／offset的数值及单位仍由原调用者交付；不重算布局、不扩大权限、不增加算术、索引检查、地址表或运行链。根地址准备、Frame地址请求、存储地址查询及真实ViewLoad／StoreIndexed各保持自己的生成或源码职责，不能把整个访问流程统一改标寻址。

原常量折叠、去重和消除继续按实际语义；没有保留的节点不补造，不为观察保活变量或冻结优化，完整变换来源与机器映射继续归原编号。材料只由compiler角色的原Inst及FunctionBody／模块持有，沿SDK／CLI来源快照消费，在最后IR消费者后结束，不进入必要执行编码、不拥有存储，不增加Frame、采集或释放链。静态地址算术不证明实际访问、地址有效或设备完成。

#### CPU写区地址准备的生成来源

application_memory的原句柄写分支仍以index、DataStride、可选relative及实际components计算原写区offset，再交给原Frame地址请求。仅该生产者的步长／字段常量、拓宽Cast、索引乘加、relative、分量偏移及请求常量共享DatasetWriteAddress，JSON reason为dataset_write_address。节点source_site为空，可选Syntax直接借用准备发射时的原当前SourceSite，语法种类、范围、定义／特化实例及inlined_at／domain_path保持，未知为null；旧拓宽节点未存source_site不等于当前Builder语法未知，不从根定义或邻近行猜补。

普通写、数组相对位置和实际外部C ABI位置借用沿同一原准备交接；初始化Store仍保留DatasetInitializeWrite及其原factory触发，本准备不替换它。每个分量的实际字节宽度及Frame请求按原语义保持，不能因返回同一根地址删除必要逐分量请求。原数值宽度、单位、求值顺序、布局、权限和最后消费者保持，不增加写区、复制、同步、检查、Frame、状态表或运行链。已是U64根的路径及公共返回／读区准备归原其他叶，不整体改标本项；原StoreIndexed、Frame地址Call及根／索引准备各保留自己的职责。

没有实际分量或未进入该分支时不虚构准备，原常量折叠、去重和消除继续生效，不为观察保活指令或冻结优化；完整变换来源与机器映射仍归原编号。材料只由compiler角色的原Inst及FunctionBody／模块持有，沿原SDK／CLI来源快照消费，在最后IR消费者后结束，不进入必要执行编码、不拥有存储或运行地址。静态写区计算不证明实际写入、地址有效或设备完成。

#### CPU访问布局与读区准备的生成来源

application_memory只为原地址型U64根分支的声明步长／零字段、返回零索引，原读句柄的零offset／stride和count请求常量，以及参数布局公共返回的零索引，共享DatasetAccessLayout，JSON reason为dataset_access_layout。这些均是原实际Constant；已有DataStride::Parameter寄存器不产生额外操作，公共返回准备也不因请求为写而归入DatasetWriteAddress。常量的原Scalar、位模式、单位、求值顺序及地址／索引／步长三元组保持，未产生的布局步骤不补造来源。

节点source_site为空，可选Syntax直接借用原发射时的当前SourceSite，语法种类、范围、定义／特化实例和inlined_at／domain_path保持；未知为null，不从资源根或邻近语句猜补。原Frame地址Call、根地址表示、索引／步长寻址和句柄写区准备仍保留各自来源；真实ViewLoad及已有DatasetDropElementRead、StoreIndexed及初始化写不改标。仅取length、没有实际载荷读取或无分量值时不为观察生成读区请求。

材料只由compiler角色的原Builder／Inst、FunctionBody／模块及SDK／CLI快照持有，最后实际IR／来源消费者结束后释放；不进入必要执行编码、不保活变量或阻止原优化合并／删除常量。真实地址、布局、权限、运行存储及完成继续由原Ty／Data、Frame和最后实际消费者持有，不新增读回、访问、检查、状态表、同步或释放链。静态准备事实不证明已读取、地址仍有效或GPU完成；完整优化来源及机器映射继续沿原编号。

#### 字段布局寻址准备的生成来源

application_reference_offset在原DataReference／DataLayout上准备的声明零偏移、非零字段常量及原IAdd，共享DatasetFieldLayout；application_reference_layout的原声明步长常量持有同类来源，JSON reason为dataset_field_layout。原Ty及真实字段路径仍是布局权威；参数布局中的stride／offset直接使用原寄存器，不为来源材料增加Constant或改写原值。字段偏移为零时不伪造字段加法，声明步长仍沿原size／align计算。

节点source_site为空，可选Syntax直接借用各原发射接点的当前SourceSite，语法种类、实际范围、定义／特化实例及inlined_at／domain_path保持；缺失触发为null，不从字段名、类型声明、资源根或邻近位置猜补。原字段字节单位、宽度、对齐、uniform及Proof保持。根／寻址／Frame地址调用、读写准备、资源参数接收／打包以及投影零索引各保留自身职责，真实ViewLoad／StoreIndexed不改标；本材料不解释为运行布局、地址有效性或设备完成证明。

生成材料只归compiler角色的原Builder／Inst与FunctionBody／模块及SDK／CLI来源快照，在最后实际IR消费者后结束；不进入必要执行编码，不拥有存储或创建第二布局表、地址图、状态、读回、同步、释放链。原常量合并／消除和指令优化照常进行，不保活变量或固定保留节点数；完整变换及机器映射继续沿原编号。

#### 投影地址选择准备的生成来源

application_projection_address只为自身原实际U32零常量交接DatasetProjectionAddress，JSON reason为dataset_projection_address。原寄存器仍作为索引与步长交给offset_application_address，实际根、字段offset、类型、位模式、字节单位和地址结果保持；根／字段布局／偏移指令各保留自身生成职责，不按组合名称重复改标或新增准备。

现有唯一消费者是create_application_view以CPU dataset的DataReference构造只读view的分支。原完整初始化、读取许可、真实步长、CPU驻留、借用及最后消费者保持；复制既有view不调用本准备，区间截取的自身偏移也不整体改标。C ABI仍不直接接收view，不将普通外部位置引用当成本函数消费者。

节点source_site为空，可选Syntax直接借用原零常量发射时的当前SourceSite，语法种类、实际范围、定义／特化实例及inlined_at／domain_path保持；未知为null，不从资源根、字段名或邻近位置猜补。原优化照常合并／消除零常量，只有实际保留的节点才在最终IR出现，不为观察保活；多条被合并来源及被消除关系继续沿原优化来源叶项交付，不能用一个剩余节点补猜全部历史。

材料只归compiler角色的原Builder／Inst、FunctionBody／模块及SDK／CLI来源快照，最后实际IR消费者后释放；不进入必要执行编码，不拥有运行存储、地址或view，不增加地址图、状态、查询、调用层、读回、同步或释放链。静态准备不证明运行读取、地址有效或GPU完成。

#### CPU与GPU资源传输的生成来源

application_transfer在原upload／download分支、所有权／活动借用／源位置检查之后，仅为既有application_call发射的实际Call交接DatasetUpload或DatasetDownload，JSON reason分别为dataset_upload及dataset_download。方向直接来自原source分支，原Frame、根句柄、参数、求值顺序和调用目标保持；普通同名函数不按符号字符串标记。编译期随后沿原Data拥有树更新residency的关系不产生Inst，不虚构别名更新指令。

节点source_site为空，可选Syntax直接借用原传输Call发射时的当前SourceSite，语法范围、定义／特化实例及inlined_at／domain_path保持；未知触发为null，不按资源名或邻近指令猜补。原编译拒绝及运行错误仍由原检查和Frame交接，不因来源放宽访问、改变借用或增加传输、读回、同步。

材料只归compiler角色的原Builder／Inst、FunctionBody／模块及SDK／CLI快照，最后实际IR消费者后释放；不进入必要执行编码，不延长运行存储寿命，不强制节点保留。真实存储、传输、依赖、最后消费者和释放继续归原Frame／blcompute；静态来源不证明已经上传、下载或GPU完成。完整优化来源与机器映射继续沿原编号。

#### 应用发起整域计算的生成来源

call_domain只为原application_execute交付Frame及实际参数后发射的Defined Call交接ApplicationDomainCall，JSON reason为application_domain_call。真实CPU／GPU目标和被调实例继续来自原CallTarget，参数与原寄存器／布局继续来自CallArgument；普通CPU调用及被调域包装内部的Call不按名称字符串改标。显式target、CPU普通整域调用及应用内提取parallel都沿原实际发起点交接，原权限、驻留、初始化、长度、形状、参数顺序及临时实参结束规则保持。

节点source_site为空，可选Syntax直接借用原Call发射时的当前SourceSite，原语法范围、定义／特化实例及inlined_at／domain_path保持；未知为null，不扩大为没有取得的target参数列表或整条parallel。参数组装、初始化模型及已有寄存器直传没有独立Inst时不虚构节点；已有字段偏移准备保留DatasetFieldLayout，不随本Call重标。原domain_at／build_domain及分段提交的内部来源另归对应叶项，本项不把根调用材料扩成整个被调包装的来源。

材料只归compiler角色的原Builder／Inst、FunctionBody／模块及SDK／CLI来源快照，最后实际IR消费者后释放；不进入必要执行编码、不保活变量，不持有World／AST或运行资源。真实计算、提交、依赖、存储及最后消费者仍归原Frame／blcompute，静态Call不证明已经执行或GPU完成；不增加调用、调度、同步、状态或释放链。完整优化／机器映射继续沿原编号。

#### 整域包装入口的生成来源

原build_domain只在同一包装body.generated_origin交接DomainEntry，JSON reason为domain_entry；其ValueCode及FunctionBody名称、参数／结果、寄存器、内部Inst、真实源定义和运行合同保持。该入口说明整域包装的生成职责，不把它扩成每条内部操作的来源，也不将包装执行名称当作原B函数或原调用实例。这些body是主机CPU包装，原符号的CPU／GPU域目标说明所发起的计算位置，不表示该body已经是GPU中间表示或最终ISA。

锚点统一为GenerationAnchor::DomainEntry { context, expansion }，JSON kind为domain_entry。context消费原DefinitionSyntax与实际Instance：Source采用Definition锚点，definition为原d.source_origin快照，instance为原Instance.name，不从最终domain符号拆解；Generated已有SourceSite时共享Syntax，保留原语法、定义／实例、内联／域路径及实际范围；CompiledInterface或未知上下文为null。expansion共享原完整Program.generated_origin，缺少展开材料时为null。trigger()及syntax()／origin()／instance()只沿context，不能用展开中的某个条件替代入口语法。实际已编译实现仍沿原domain_at装入，不重读其实现或因缺少源码生成另一份包装IR。FunctionBody.source_origin继续只记录真实原定义，Generated入口不因此获得虚假源码身份。

材料只归compiler角色的原ValueCode／FunctionBody／模块及SDK／CLI快照，在最后实际IR消费者后释放；原body克隆共享不可变来源，不持有World／AST、执行资源或第二登记表。关闭compiler角色不包含该类型／字段，必要执行编码、B语法、C ABI与资源完成保持。空指令body仍可以具有真实生成入口；原优化照常，不保活变量或节点。内部原Call／字面量准备／分段提交来源各归原职责，真实CPU／GPU计算及最后完成仍归Frame／blcompute，静态入口不证明已运行。

#### 阶段整值Name的绑定来源

stage_root从原env或模块const取得Literal时，用同一Root::reference_literal交接实际Name引用；stage_expression仍经过原注解、类型与完全常量检查，成功结果的原表达式本身为Name时复用同一构造。Root保留原Ty／Bits，已有生成材料进入retained。GenerationReason::StageConstantReference表示这次真实编译期绑定，GenerationAnchor::StageConstantReference { reference, retained }及JSON kind stage_constant_reference分别共享原Expression SourceSite和原GeneratedOrigin，缺失各自为null；单源trigger()、syntax()／origin()／instance()只解释reference，不能用被引用常量定义或retained替代本次读取。它不是算术折叠事件；Data／Uniform／Texture／Acceleration以及直接Number／StaticU32不产生常量Name引用。

expand_stages将原domain_path经原stage_root／stage_expression参数交给临时Builder.source_context，定义、实际实例与inline关系保持。阶段表达式及其原操作据此保存实际域路径；模块const的求值仍拥有自身定义上下文。根级为空，嵌套域只使用原DomainSite序列，不从最终函数名推导调用路径或复制图。

Builder::roots仍以原evaluated_constant形成相同Constant、Scalar／Bits及寄存器，新绑定材料和原求值材料一起进入同一Inst及原CSE／DCE／空Map根；直接source_site按原生成职责保持，原绑定来源从generated_origin查询。DomainCallLiteral.evaluation、阶段if及for边界复用原Root材料交接，不新增消费者或另一来源字段。临时求值的实际结果可经原StageConstantEvaluation.operations持有输入已有绑定，不伪造额外算术或完整逐输入因果图。材料仅compiler，随原Root／IR／模块及快照到最后消费者，不保活操作、保存旧Builder／IR／World／AST、增加必要编码、运行状态或资源寿命。

#### 阶段if选择的生成来源

expand_stages在原bool检查和唯一所选body成功展开后，把StageIfSelection交给同一Program.generated_origin。GenerationAnchor::StageIfSelection及JSON kind stage_if_selection分别提供condition（原Expression SourceSite）、value（本次bool）、evaluation（原Root可选求值GeneratedOrigin）、returned（该次阶段展开是否早退）、stages／calls（本次实际新增Stage及DomainCall的OperationOrigin）与retained（此前Program根，含递归内层选择）。stages按实际展开顺序共享原operation_source及generated_origin，calls按实际生成顺序共享原call.source而不造求值事件；二者不是最终融合阶段编号或运行次数，未选分支没有产生这些结果。单源查询只解释condition，evaluation和retained独立读取。空分支、无新阶段或阶段return仍有选择事实；失败不形成成功记录，不制造指令、复制IR、保活阶段或改变原初始化／返回语义。

原Program查询持有完整选择材料；原生整域FunctionBody通过DomainEntry.expansion共享同一根，空body及仅调用已编译域也能查询。物理片段Program不继承完整展开根，其自身优化只记录实际片段；DomainPartCall继续仅使用入口context，不能把全域选择写到每段。材料为compiler数据，随原IR及快照到最后消费者，不进入必要对象／执行编码、拥有运行资源或增加状态表。最终融合来源及机器映射仍沿原任务推进，完整展开须查询原Program或整域FunctionBody，单片段查询不代表全域。

#### 阶段for展开的生成来源

原bound闭包仍各求值一次并按原规则要求u32。StageForBound在compiler配置公开导出，expression()借用原Expression SourceSite，value()为本次实际u32，evaluation()共享原Root已有GeneratedOrigin；JSON kind为stage_for_bound，expression／value／evaluation分别交接，材料缺失为null，不重求值或改变范围检查。

成功展开后，原Program.generated_origin持有StageForExpansion；同名GenerationAnchor及JSON kind stage_for_expansion分别提供statement（原Statement SourceSite）、counter（原计数器名称）、first／last（两个StageForBound）、iterations及retained（循环前已有根）。范围仍为原first.value..last.value，空／反向范围不产生迭代或阶段，但保留两个实际边界；单源查询只解释statement，不拿某个边界替代整个循环语法或猜补源码范围。

StageForIteration同样从原ir公开导出，index()／returned()给出该次实际下标和阶段早退，stages()／calls()分别借用本次新增Stage的operation_source／generated_origin及DomainCall的source所形成OperationOrigin，expansion()为本次body的原展开根；JSON kind为stage_for_iteration，字段同名。循环前根直接交给循环retained，各次body根移入自身iteration.expansion，不让此前迭代的材料重复进入后续迭代。内层if／for、域路径及其原求值材料保留明确归属，不存Stage／Call IR、寄存器、旧Builder或另一控制图。空body有实际迭代而没有Stage；提前return只保留实际已经展开的迭代，失败不交接成功结果。

材料沿原Program、DomainEntry.expansion及同一模块／快照到最后IR消费者，物理片段继续只持有自身事实；不新增入口、运行参数、状态、必要编码或资源寿命。迭代记录解释编译展开，不是运行次数、最后融合的Stage身份、设备完成或性能读数；原优化不为观察保活。

#### 整域调用字面量实参的生成来源

build_domain只在原Root::Literal分支为逐分量准备的Constant共享DomainCallLiteral，JSON reason为domain_call_literal；原Ty.components与words顺序、Scalar、Bits、寄存器和CallArgument保持。该材料区分编译器为已编译域Call准备的实参，不按数值或符号把普通常量改标，也不为Root::Data／Root::Uniform的寄存器直传补造指令。

节点source_site为空。GenerationAnchor::DomainCallLiteral { call, evaluation }分别共享原call.source与同一Root可选的已有GeneratedOrigin；每个成员独立以null说明该事实缺失，所有本角色准备常量使用同一表示。原GenerationReason::DomainCallLiteral保持，JSON reason仍为domain_call_literal，新锚点kind也为domain_call_literal，成员为call（原source_syntax或null）及evaluation（已有generated_origin或null）。

原GeneratedOrigin::trigger()及锚点syntax()／origin()／instance()仍只借用call；调用语法、原定义／实际实例及内联／域路径保持。evaluation表示本次Literal结果已有的StageConstantReference及其retained中的ModuleConstantEvaluation、StageConstantEvaluation等事实，不替代调用触发，也不从调用范围猜补求值表达式；没有调用语法时相关单源查询为空，求值事实仍从evaluation取得。每个实际Root的准备Constant继续共享一个原生成材料，Scalar／Bits／分量／寄存器及CallArgument顺序不变。真实已编译域Call仍使用同一原source_site，入口body、其他准备及分段运行Call保持自身职责；不改调用、求值顺序、精度、布局或参数ABI，不为Data／Uniform直传造值准备。

材料只归compiler角色的原Inst／FunctionBody／模块及SDK／CLI快照，最后实际IR消费者后释放；原优化照常，不保活常量，不进入必要执行编码，不持有World／AST或运行资源。真实计算、Frame存储、依赖及最后完成沿原所有者，不把静态常量准备当作实际执行；不增加值表、图、调度器、同步或释放链。

#### 域分段运行Call的生成来源

原emit_domain_part仅在原实际runtime_call发射点交接DomainPartCall，JSON reason为domain_part_call；原Call种类、目标、Frame／Data根与字节偏移、uniform、顺序及结果保持。build_domain共享原DomainEntry材料，两个原helper交接点只借用其anchor，原包装body仍持有同一入口材料。节点source_site为空，Source采用同一原Definition快照与实际Instance，Generated直接共享已有Syntax；未知保持null，不从part／run_index或最终执行名称重建源码身份。

来源描述主机CPU包装的分段发起职责，CPU／GPU域目标、实际Stage子区间及前缀要求／保证仍归原Program与Module资源目录中的程序引用；各Stage原来源保持，入口上下文不代替逐Stage／指令映射，也不表示GPU最终ISA。空Stage区间仍直接返回，不为观察制造Call或程序；真实已编译域Call仍保留自身call.source及域路径。

材料只归compiler角色的原Inst／FunctionBody／模块与SDK／CLI快照，最后真实IR消费者后释放；不进入必要执行编码、不保活变量或节点，不持有World／AST或运行资源。Frame／blcompute继续承担原存储、依赖、执行与最后完成；静态Call不证明计算已运行。原优化、资源所有权及错误保持，不增加调度、图、同步、第二状态或释放链。

#### Stage Dataset调用实参准备的生成来源

native_call只在原Argument::Dataset分支的零索引Constant、PositionAddress、BufferStride及字段偏移Constant四类真实发射接点共享DatasetCallArgument，JSON reason为dataset_call_argument。节点source_site为空，Syntax直接借用原发射时self.source_site，保留原语法、定义／实际实例及内联／域路径，未知为null；触发只是已取得的调用范围，不扩大为完整参数列表。原实际Call的source_site保持，已有根／长度寄存器和descriptor／CallArgument组装不因观察补造Inst。

原Scalar／Bits、零索引、真实DatasetView绑定、字段字节偏移、声明步长、位置与根Proof、Access、形状、参数顺序及ABI保持。该生产者对应CPU阶段对已编译函数的实际调用；源码函数内联及GPU路径各自沿原生产者，不按函数名或表示名称改标。read／atomic／inout原许可及真实检查保持，不以来源分类放宽CPU／GPU目标或写入权限。

材料只归compiler角色的原Inst／ValueCode／FunctionBody、模块与SDK／CLI快照，在最后真实IR消费者后释放；不进入必要执行编码、不保活变量或节点，不持有World／AST或运行资源。真实存储、借用、依赖和最后完成仍沿Frame／blcompute及实际消费者；静态准备不证明调用已运行。原优化、错误与合法动态输入保持，不增加参数／状态表、图、调度、同步或释放链。

#### CPU借用描述符步长Cast的生成来源

application_reference_value只在原descriptor需要U64步长而实际布局步长尚不是U64时，为原实际Cast交接DatasetBorrowStrideCast，JSON reason为dataset_borrow_stride_cast。当前U32步长拓宽为U64，原dst／src寄存器、数值转换、stride的Proof／constant／uniform及descriptor组装保持；已有U64的描述符步长仍直接交接，不补造Cast。application_stride_parts将view实际步长编码为两个U32：实际Cast、32位移位常量、逻辑右移及低／高Cast使用同一生成原因；仅转换表示，不改变步长值或资源来源。

节点source_site为空，Syntax直接借用原发射时SourceSite，原语法、定义／实际实例及内联／域路径保持；未知为null，触发仅为已取得的原范围。storage_address、Projection根Proof、原字段布局、普通数值或其他寻址Cast不因共享语法触发而改标。原Call、根／长度／字段偏移、参数顺序、访问许可、借用、ABI与错误保持；步长表示拓宽不改变实际字节步长或复制数据。

材料只归compiler角色的原Inst／ValueCode／FunctionBody、模块及SDK／CLI快照，最后真实IR消费者后释放，不进入必要执行编码、不持有World／AST或运行资源。Frame及真实最后消费者继续负责存储、依赖和完成；不建立参数／状态表、图、调度、同步或释放链。原优化继续折叠／消除，不为来源强制保活变量或Cast，完整变换与机器映射仍归原编号。

#### 普通函数形参与IR输入的来源

compiler角色的原CallParameter将实际值／引用种类保留为CallParameterKind，并在同一记录持有可选共享ParameterOrigin。CallParameter::value／reference或new默认没有来源；原模块合同仅编码种类、Ty与Access，解码不恢复或伪造提供者实现来源，原0／1标签与字节格式保持。关闭compiler不编入来源字段与接口。

function_parameters从原Definition、形参声明顺序及当前实际函数实例生产材料。ParameterOrigin::Frame在JSON为implicit_frame，与普通B形参分开；Parameter保存原零基index、name、实际instance、generated及可选site。真实Source形参在JSON为source_parameter，site沿原Param.span交接parameter语法、定义及既有内联／域上下文；Param.span只覆盖起始令牌，不扩大为整个声明。合成定义为generated_parameter，site借用原生成触发而非伪造参数声明；CompiledInterface不取得实现来源。没有原ABI输入的纹理／阶段绑定不制造CallParameter，原域包装未交接本叶参数材料时origin保持null，不从同名参数猜补。

CallParameter::origin()只借用同一不可变材料，ParameterOrigin::ir_json()序列化该事实。FunctionBody::parameter_inputs()按原参数顺序返回借用的(CallParameter, inputs切片)，值的宽度取真实Ty.components，引用为原两个U32地址分量；函数IR的每个parameters条目同样输出origin及input_registers。一个B向量引用可以对应多个实际参数并共享来源，零分量值保留原参数身份且输入为空；Frame省略时材料和输入随原记录一起移除，不保留假槽位。切片直接取原ValueCode.inputs，不另存参数槽位表或改变输入编号。

这里是逻辑IR ABI关系，最终Windows寄存器、栈、隐藏机器输入与指令映射仍由原后端来源任务交付。必要运行编码不带这些材料，参数、类型、实际值、借用、Frame及最后消费者的原权威保持；材料沿原FunctionBody／模块、SDK／CLI快照共享，在最后IR消费者后释放，不保留AST／World、不拥有运行资源、不保活变量或指令，不建立状态、图、调度或释放链。完整优化的Release仍按原规则执行。

#### 普通调用B实参与实际CallArgument的来源

compiler角色的原CallArgument值／引用变体直接持有可选共享ArgumentOrigin，CallArgument::origin()借用同一材料，ArgumentOrigin::ir_json()交接结构事实。原Call JSON的arguments每项增加origin：隐式Frame为implicit_frame_argument；普通B参数关系为parameter_argument，包含被调B形参零基index、name及可选site。实际被调实例继续只归原Call.target，不另存目标绑定；这些是逻辑IR实参，最终机器寄存器／栈、隐藏输入及指令映射仍由后端任务交付。

普通／泛型调用与CPU内联中的递归调用沿原调用链临时借用同一次原Expr列表，在native_call实际准备ABI实参时按原形参次序取得Expr.span，site沿原Builder产生expression语法及调用者定义、实际实例、内联／域上下文；不重新求值，也不把Call本身的source_site借作实参位置。Expr.span只表示解析器实际取得的范围，例如嵌套调用通常是其调用标识符，不扩大为整个表达式。多个引用分量共享同一B实参材料；零分量值仍保留原参数关系，values为空。Frame裁剪直接删除同一原实参及其材料，不造成后续参数序号漂移。

没有原实参语法的根入口、生成的dataset初始化／释放及自定义值核参数明确不提供Expr列表；如果形成B原生调用，已知参数序号／名称仍可交接，site为null，不将null推断成某个源码表达式或特定生成原因，原Inst.generated_origin继续说明已知生成职责。C调用、运行库及域包装等其余原构造默认origin为null，不按类型名、符号名或最终寄存器猜补。提供者源码／头文件缺席的已编译调用仍从原公开接口确定参数次序／名称，已取得的site归调用者自身源码，不伪造提供者实现；实际形参声明／输入关系仍见上节。

材料只由原CallArgument／Call／Inst、FunctionBody／ValueCode、模块及SDK／CLI快照持有；克隆与原寄存器／局部重映射保留同一不可变事实，最后真实IR消费者后释放，不保留Expr／AST／World、不拥有运行值或资源。关闭compiler不包含该字段／类型／接口／JSON字段，必要对象／执行编码不增加观察材料；对象及资源映像版本由各自真实编码合同决定，观察字段不独立升级或冻结格式。原求值顺序、类型、布局、借用、Scoped资源结束、Frame、数值效果与最后消费者不因观察改变，不增加参数表、状态、图、调度、同步或释放链，不为来源保活指令。

#### 数组形参物化StoreLocal的生成来源

materialize_array_bindings只为原含数组Binding::Value的实际物化交接ArrayParameterStore，JSON reason为array_parameter_store。同一原materialize_array逐分量StoreLocal共享该材料并创建新Local；materialize_array_bindings在原按值参数绑定处用现有read_only形成ReadOnlyLocal。原槽位／storage、源寄存器、参数只读权限、求值顺序、Proof及初始化保持；无实际StoreLocal的零长值不补节点，原优化可以消除常量索引或未使用存储，不为观察保活。

Source定义按同一原B参数身份取得Param.span，由原Builder.source_at生成SourceSyntax::Parameter，JSON syntax为parameter；当前解析器只保存该参数起始令牌，不能扩大为整条声明。SourceSite保留原SourceOrigin、真实特化实例及inlined_at／domain_path；环境遍历顺序与原形参声明顺序不同，也须按原参数身份交接范围。Generated只借用已有生成Syntax；未知与CompiledInterface不伪造参数声明。节点source_site为空，触发只归generated_origin，不把这些生成Store当成用户直接赋值。

materialize_array共用算法只接收可选来源，表达式、局部绑定、取址、外部／普通实参、拥有／view暂存继续原来源；不能因包含数组就全改标为形参，也不能按最终寄存器号猜原参数。来源不改变参数ABI，不代替B参数到实际输入槽的关系。已编译消费者未保留提供者原IR时不重建或伪造实现；原提供者编译结果及消费者自己的真实IR分别查询。

材料只归compiler角色的原Inst／ValueCode／FunctionBody、模块与SDK／CLI快照，在最后真实IR消费者后释放；不保留AST／World、不进入必要执行编码、不持有运行资源，不建立参数／状态表、图、调度、同步或释放链。Frame及真实消费者继续负责原资源、借用与最后完成，完整变换及机器来源另归原编号。

#### 集合值核的入口与捕获来源

compiler功能的`ValueCode::entry_origin()`返回可选只读`ValueCodeEntry`；原CustomReduction的project、identity和combine分别持有自己的入口，角色由原字段表达，不另存角色表。`definition()`借用目标函数的可选原SourceOrigin，`instance()`借用原specialized_arguments实际产出的完整Instance.name；二者在原invoke_returned消费实例之前取得，不从内部剩余指令、根程序名或字符串重新解析。`reference()`借用调用方原SourceSite，当前静态引用Expr.span通常为引用标识符，定义／实例属于真实调用方，不扩大为未取得的泛型参数或bind完整表达式。

`capture_parameter_start()`返回原元素参数数量，表示第一个捕获对应的B参数零基下标；一个向量／记录参数展开为多个分量时仍只计一个B参数。`captures()`按原bind参数顺序返回具有精确长度的借用迭代器，每项为可选SourceSite；各项沿同一Builder原定义和实例上下文取得原捕获Expr.span，定义、引用和各项范围独立缺失。实际捕获值、寄存器、类型与求值效果仍由原ValueCode及运行消费者解释，不在入口材料复制参数值或假称动态值编译期已知。

value_ir增加entry_origin：没有集合入口为null，否则kind为value_entry，含definition（source_definition或null）、instance、reference（source_syntax或null）、parameter_index_base=0、capture_parameter_start及captures数组。数组各项kind为value_capture，parameter为原B参数下标，source_site为原捕获位置或null；空数组表示没有捕获，不表示材料缺失。普通函数体、应用主入口和域调用包装没有本项集合入口，明确为null，原FunctionBody／Program及Inst来源分别保持。

来源由原ValueCode持有不可变Arc，克隆共享同一入口；目标与调用方Source文本沿原快照共享，原IR最后消费者后释放。空代码、返回输入本身或折叠后的值核仍保留真实入口，后续文件改变／移走不修改已取得材料；不保活已消除指令，不改变优化或内联选择。Rust的compiler ValueCode新增内部字段，消费者沿实际编译结果和原克隆取得，不依赖旧结构字面量／布局。关闭compiler时类型／字段及entry_origin输出均不存在；ExecutionPlan、BLRI必要编码、资源所有权与完成、B语法和C ABI均不因本项改变。域展开、优化多对多来源与机器映射仍沿原叶，不以本项冒充完整运行调用图。

### 保存和恢复可选CPU范围材料

compiler Rust接口的NativeCode.sources统一表示三种互斥状态：Emitted持有原可选operation_ranges与function_abi，Restored持有源文本之外的SavedSources，Unavailable表示执行解码没有来源。原两个顶层字段由该字段替代；code_json的function_abi与operation_ranges输出模式及未知／空列表语义保持。原Emitter::finish一次交接ABI和操作材料；COFF按值接收同一sources，BLCR恢复替换原来源时释放其Source引用，不维护第二份权威。保存只借用原事实，恢复后的重复保存产生同一材料；执行编码和运行库均不消费这些字段。

原新生成的`build::Compilation`可显式调用`code_observations()`取得独立字节，由调用者保存为`.blcr`；`resource::Module`提供同名方法。只读捕获不修改执行文件、构建键或缓存。当前SDK是捕获入口，`bc build`的`.x64.json`不等同这份材料；未保存材料的缓存结果不能重新导出范围，不为取得材料自动重编。正式执行文件不要求携带`.blcr`，部署映像仍由`build::execution_image`／`bc image`交接。

BLCR7由8字节`BLCR\x07\0\0\0`、u64条目数量及逐条记录组成；整数均为小端。每条记录依次为32字节SHA-256、u64尾部分区数量及各分区的u64排他末端／u32类别（0探栈辅助、1对齐、2字面量），再是可选尾部引用、可选FunctionAbi及可选OperationRange序列。可选值以一字节bool标明是否存在，序列以u64数量开头；每个TailReference含u64 target及u64数量的rel32字段位置序列。ABI、操作、定义／声明／语法及变换记录与下文BLCF10共用同一native/facts/material字段编码；只读SavedSources不含Source或源码文本，原未知与已采集空列表分别保持。BLCR1至4不再受支持。摘要消费同一NativeCode的原执行字段编码，覆盖原字节、重定位、帧、调用、主末端和展开，不含观察材料、源码、SDK版本或工具链身份；材料不复制机器码、执行图或主末端。它配对同一执行编码，不能证明材料来源、指令安全、硬件支持或真实执行，调用者应保留原生产材料。

`restore_code_observations(material)`按原容器顺序消费全部CPU代码：Compilation先programs后templates；Module按原prefix合同将无前置条件映像置前、条件模板置后，各组保持原顺序，与构建结果一致。每个CPU映像沿原阶段，Element先main再实际延续入口，Sort先全部initialize再histogram／scatter／gather，再交接原functions命名callee及同一资源目录的CPU子映像阶段／callee；GPU不占CPU条目。恢复先核对数量、全部执行字段摘要、主末端、尾部分区的范围／顺序／完整覆盖、实际rel32字段与目标、ABI及操作范围；全通过才将tail_regions、tail_references及CodeSources::Restored移动回同一NativeCode；错误不部分改写。原字节、执行／资源合同、GPU内容和最后消费者保持；纯GPU集合明确不支持CPU观察。

```text
bc code target/cache/entry.blbi --observations target/entry.blcr
bc code --image target/deployment.blri --observations target/entry.blcr
```

SDK由`build::code_facts(path, Some(material))`或显式BLRI模式的`build::image_code_facts(execution_bytes, Some(material))`交接同一JSON；纯GPU不需要CPU范围材料，传None并省略CLI的--observations即可。CLI在Compiler初始化前只读装入原编码，显式提供CPU材料时沿原恢复，不读取B源码、不调用编译器／驱动、不分配可执行内存或运行代码；读取、编码、错配及范围错误直接失败，不输出部分成功。BLBI结果kind=build_code，programs／templates保留各原索引；BLRI结果kind=execution_image_code，images保留原索引。代码与源码范围查询的artifact_sha256是该次实际读取的完整BLBI或BLRI字节SHA-256；提取出的BLRI有自己的产物身份。CPU项target=cpu，code为原native::ProgramImage::code_json；GPU项target=gpu，code为原spirv::ProgramImage::code_json，各模块独立ID／偏移。材料仍只配对CPU代码，纯GPU误提供CPU材料明确失败；CPU缺失材料不猜范围。所有代码地址／模块偏移均为相对值，不代表装载现场。普通COFF／最终PE、源码来源及实际装载地址分别沿C-021.7.9.5.3／.9、.7、.13交付。


### CPU映像机器区间反查原来源

```text
bc code target/blink/entry.blbi --observations target/ranges.blcr --source-range 32:48
bc code --image target/deployment.blri --source-range 32:48 --observations target/ranges.blcr
```

compiler Rust SDK可对原`NativeCode::source_json(start..end)`或`ProgramImage::source_json(start..end)`直接查询；保存后用`build::source_facts(path,start..end,Some(material))`及`build::image_source_facts(execution_bytes,start..end,Some(material))`，无材料传None。CLI沿原code／code --image入口增加source-range选项，与observations可任意先后；不加source-range时仍为原完整机器代码／GPU模块查询。只有读取原BLBI／BLRI及可选BLCR，不读B源码、创建Compiler、重新生成代码、分配可执行内存或运行CPU／GPU。

范围单位为字节且为半开区间，每个原NativeCode各用自己的字节零；它不是整个程序的共同装载地址、某个函数符号的新起点、RVA或文件位置。start／end为十进制usize；反向范围报错，空范围没有命中，超出或完全位于代码外的范围不钳制、不推断来源。原ABI／操作／分区只按实际字节交集筛选，保留原完整区间、顺序、嵌套与全部原来源，不解码或裁短指令，不只取首项；零长度没有机器字节，不作为命中，无命中不证明已消除。

NativeCode结果kind=native_source_ranges、address_basis=image_relative，保留range_unit=byte、range_kind=half_open、请求start_byte／end_byte及原byte_length／function_end；regions返回原main_function／stack_probe／alignment／literal中的相交分区及原start_byte／end_byte／byte_length，function_abi保留定义／实例上下文并筛选ranges，operation_ranges保留相交操作记录及其完整origin。定义／实例只说明所属上下文，不作为机器命中。Emitted直接借用原IR来源，Restored复用同一SavedSources筛选；不投影另一来源树或重解析JSON。原字段未保留或没有材料时为null，已采集序列筛选无相交为空列表，未知origin仍保持未知。

ProgramImage结果kind=native_program_source_ranges、address_basis=each_image_relative；program、stages、functions及resource_images沿原完整查询的真实容器结构，source_range交接同次请求及单位／半开语义。Element的main、全部延续入口、Scan的prefix／apply、Sort的全部initialize／histogram／scatter／gather及具名函数分别交出上述NativeCode结果；不会因多个入口都从0起而合并它们。CPU资源子映像递归复用同一原结构，保留原索引／target；GPU项的code明确为`{"status":"not_supported"}`，不输出伪GPU来源或借用完整SPIR-V指令冒充来源。原module_code不存在时为null；存在但尚无来源材料时返回section_relative及not_collected，不丢失这个存在事实或从物理代码猜来源。这些状态是本次只读结果，不增加运行状态、来源注册表、索引或资源拥有者。

保存BLBI结果kind=build_source_ranges，programs／templates保留原索引和target；BLRI结果kind=execution_image_source_ranges，images保留原索引和target。两者同样交接range_unit／range_kind及请求start_byte／end_byte；CPU的code为上述ProgramImage来源结果，GPU项明确not_supported。提供BLCR时先沿原整组身份／范围／引用及Source字段恢复，错误直接交接，不输出部分匹配；无材料的CPU查询仍返回真实代码长度／容器上下文和null来源，而原完整CPU机器查询仍保留其缺少尾部分区的错误语义。执行编码、BLCR7／BLCF10及最后消费者的资源寿命保持；返回字符串归调用者，不延长原Source／IR或实际CPU／GPU消费者。源码范围正查仍沿.6.3／.6.4待实现，真实装载地址仍沿.13。

### 已保存COFF的物理结构与代码查询

```text
bc code --object target/module.obj
bc code --object target/module.obj --section 1
bc code --object target/module.obj --function exact-qualified-symbol
```

Rust SDK使用`object::code_facts(bytes, object::CodeSelection::CodeSections)`，或Section(number)／Function(name)。直接消费调用者拥有的原COFF字节，复用原B `bootstrap.coff_read`的headers／section／symbol／relocation，不经过Object逻辑节映射或符号归一化，不要求Blink私有元数据／合同，不读取B源码、构建输入含义、内嵌归档成员或重编、链接、装载／执行代码。CLI沿现有code入口在Compiler初始化前分派。观察、归档、模块导入和Blink链接共用COFF符号表读取，主记录及其辅助记录须位于文件头声明的符号表范围内。节的`byte_length`表示实际文件内容长度。未初始化节需要装载零区时，`zero_fill_bytes`给出零字节数。输出是独立字符串，查询结束不保活原产物或执行资源。

结果kind=coff_code、bitness=64、address_basis=section_relative；header_bytes为原20字节COFF头。代码与源码范围查询的artifact_sha256来自该次实际读取的完整COFF字节，可与同产物的bc inputs结果关联。section_number是从1开始的原物理节号，symbol_index是从0开始、包含辅助记录占位的原符号表索引，均区别于Object内部逻辑节索引；原符号／字符串表文件位置及完整string_table_bytes保留。sections始终列出全部原节的header_bytes、特征、实际长度及文件位置；默认只为原代码特征位选择的节展开bytes／regions／relocations，--section可明确选择任意物理节。--function只按精确、唯一且有定义的原函数符号选择其所在节，保留原symbol_index／value，不推测该符号的新函数末端或截掉同节其他字节。

symbols返回全部主记录的原name／name_bytes、带符号section_number、value、storage_class、原读取器的function标记、18字节record_bytes及全部auxiliary_bytes／数量；符号文件位置与原表索引保持。节与符号名称沿现有UTF-8读取，原节名沿头内拼写，当前未展开长节名索引，不宣称支持全部COFF扩展或BigObj。relocations交接原节内offset_bytes、type、symbol_index及原目标name／section_number／value，不先规范化成另一个符号或把占位操作数当最终调用地址。原记录字节保留读取器当前未解释的字段；这些实现范围不成为语言和未来目标的永久边界。

function_tables保留原.pdata的每条12字节记录、begin／end／unwind原加数与重定位。AMD64 ADDR32NB（3）指向同一文件已定义节时，将原32位符号value与u32加数在32位RVA字段内相加，保留低32位后再按目标节解释位置；例如value=1、addend=0xffffffff与value=0、addend=0均表示该节偏移0。随后沿原目标节长度及begin／end顺序解释真实范围，不把合法回绕扩大为64位节外偏移；字段无重定位、类型未支持或非节内目标分别标明no_relocation、relocation_type_not_supported、symbol_not_section_relative。begin／end在同一节且范围有效时，regions以runtime_function交接原范围，并调用原native::instructions_json；每条指令IP按该节内偏移解释，不代表最终RVA或装载地址。跨节开始／末端标cross_section_range，待实际链接布局，不据此判定源程序非法；未解析字段的表记录标unresolved，不猜范围。

未提供生成材料时，选中节中已声明范围以外的字节全部原样交接为unclassified，instructions=null。原代码特征位、函数符号或能反汇编不证明整节都是指令；完整生成分类由下述独立材料显式交接。多个已声明范围分别保留原表来源，不据其重叠生成另一代码布局。结构截断、无效字符串／符号／重定位引用、真实范围越界／反向及ISA解码错误直接失败，不输出部分成功；解码错误保留原物理节号、节内字节偏移及相对IP。标准定义见[Microsoft PE／COFF格式](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format)。最终PE、归档成员抽取及完整静态运行库观察仍沿C-021.7.9.9，源码来源和运行地址分别沿.7／.13。


### COFF机器区间反查原来源

```text
bc code --object target/module.obj --observations target/module.blcf --section 1 --source-range 32:48
bc code --object target/module.obj --observations target/module.blcf --function exact-qualified-symbol --source-range 32:48
```

Rust SDK使用`object::source_facts(bytes, selection, start..end, Some(material))`；selection沿原CodeSelection，省略材料传None。CLI在原code --object入口增加可选--source-range，未使用它时保持原完整coff_code查询。start／end为十进制节内字节半开范围，不是源码位置、函数内偏移、文件位置、RVA或装载地址；--function仍只选择该精确符号所在物理节，不从symbol.value重置字节零，不推断该函数新末端。默认选择全部原代码节，同一范围分别应用到每节的字节零，结果保留各自上下文。反向范围报错；start=end为空查询；范围可越过节末端或完全位于其外，不钳制输入或推断来源。

结果kind=coff_source_ranges、address_basis=section_relative、range_unit=byte、range_kind=half_open；start_byte／end_byte保留原请求，selection沿原物理选择。sections仅列所选原节的section_number／name／byte_length及regions／generation／function_abi／operation_ranges；查询不输出机器字节、指令解码或另一份符号／重定位目录。regions返回与请求存在实际字节交集的原生成分区，保留原kind、start_byte／end_byte／byte_length；function_abi保留原定义／实例上下文，只筛选其ranges；operation_ranges直接筛选原操作记录。原区间、嵌套／顺序、参数、实例、内联、域及变换来源完整保留，不裁短记录、合并命中或按地址只取第一项。零长度记录没有字节交集；即使位于请求内部也不当作机器命中。

generation保留所选整节真实生成目的，function_abi的definition／instance也仅是上下文；实际命中须读对应regions／ranges／operation_ranges，不把上下文冒充所查字节的具体来源。原字段未采集／手写节没有对应字段／未提供材料为null；原已采集序列经筛选无命中为空列表。未知来源记录仍原样返回，空范围和范围外不会制造源码、已消除状态、断点或机器安全结论；原完整查询可继续观察零长度及消除材料。源范围正查仍沿C-021.7.9.7.6.3／.6.4待实现；CPU映像反查见下述独立入口，不将COFF入口写成完整双向查询。

提供材料时先由原Material读取、配对整组代码节／重定位／主范围及全部原布局，然后借用同一只读SavedSources筛选原字段；错误沿原入口交接，不输出部分材料。未提供材料时regions及两类来源字段明确为null，不从可解码性或.pdata猜生成来源。查询不读源码、重编、建立反向索引／来源树、装入代码或创建执行资源；所有临时读取和返回字符串归调用者，结束后不持有原Source／IR或运行资源。BLCF10／BLCR7及必要执行编码、材料捕获和资源生命周期保持。

### COFF原生成范围的独立材料

新生成的`Object::code_observations()`显式返回独立BLCF10字节，由调用者保存为`.blcf`；原build::ObjectCompilation.object可直接调用。SDK是当前捕获入口，不新增编译选项或为缓存命中自动重编。原Section仅可选持有生成布局：Function移动同一NativeCode.tail_regions、tail_references与sources，主末端仍由原.pdata交接；原手写生产者分别声明Startup、Print(Scalar)、ImportThunk，职责均覆盖同一整节：启动及结果交接、所选类型的标准输出、实际导入尾跳转。编码及Object::read不保存该字段，已有／缓存Object缺失原事实时明确失败；纯类型／无代码对象可以导出空代码集合。

显式捕获先确认原事实可取得，再借用同一Layout直接编码，不克隆原布局或引用；ABI定义／形参与操作的语法、声明、生成事实共用source.rs的位置投影，只在此时从原不可变Source快照取得不含文本的只读事实；原生成与变换关系按原字段直接编码，不先展开临时来源树，普通对象生成只移动原FunctionAbi与OperationRange，不提前计算摘要或位置；复用原COFF编码作一次临时序列化，沿原B读取器消费最终节及重定位，不读取源文件或编译／生成第二份机器码。临时编码字节在返回后释放，材料不含代码、源码、执行图、资源拥有表或编译器私有容器。此额外编码／摘要工作只随明确捕获请求发生；普通构建、链接、运行不写入或消费材料。链接组合后的新节及新增导入／资源数据节不冒称持有原生成材料，最终链接观察仍沿.9。

BLCF10由8字节`BLCF\x0a\0\0\0`、u64记录数量及按原物理代码节顺序排列的记录组成；整数均小端。每条为u64物理节号、32字节SHA-256、u32布局类别；0=Function，随后u64尾部分区数量及各项u64排他末端／u32类别（0探栈、1对齐、2字面量），再写u8引用可用标记（0=未保留，1=已采集）。标记1后为u64引用数量，各项为u64目标偏移、u64字段数量及各u64 rel32字段偏移；目标和字段均相对此节字节0。其后是u8 ABI可用标记，0=未保留、1=后随下述FunctionAbi记录。其后是u8操作材料可用标记，0=未保留、1=后随u64数量及OperationRange记录。1=Startup，无额外字段；2=Print，随后u32 Scalar（0=f32、1=u32、2=i32、3=bool、4=u64、5=i64、6=f64、7=u8、8=i8、9=u16、10=i16）；3=ImportThunk，无额外字段。三种手写类别均没有尾部引用、函数ABI或IR操作记录。旧BLCF版本明确不支持，调用者应保存同次原Object生成的新材料，不自动重编或兼容解码。Function不另存主末端，查询消费原.pdata中从节内0开始的同一主范围；尾部单调且完整覆盖该节。Startup／Print／ImportThunk明确覆盖0至原节字节长度，不能从代码特征位推导。

FunctionAbi按序为可选Definition、可选实例字符串、u64范围数量及范围记录。每条范围为u64 start、u64排他end和u32职责：0=hidden_result_home（u64 abi_slot），1=parameter_home、2=parameter_input（均为u64 abi_slot、u64 parameter_index、可选Parameter），3=scratch_save，4=scratch_restore，5=frame_allocate、6=frame_release（均为u32 frame_bytes），7=nonvolatile_save，8=nonvolatile_restore，9=result_address_save、10=indirect_result（均为u32 stack_offset_bytes），11=float_result，12=integer_result，13=fallible_success_status，14=function_return，15=parallel_range（随后ParallelRangePart与OperationOrigin，后者为原Stage上下文）。无附加字段的职责只写类别。实际区间可为零长度，且start≤end≤同一.pdata主末端；不得覆盖尾部、跨节或推断运行值。原COFF查询的function_abi与原NativeCode::code_json使用相同JSON字段和单位；未选中节不增加详细内容，选中但未取得材料、未保留ABI或手写节为null。

ParallelRangePart为u32类别：0=enter（u32 local_frame_bytes），1=scalar_bounds，2=scalar_selection（u64 binding_index、u32 field_offset_bytes），3=scalar_advance，4=scalar_back_edge，5=exit（u32 local_frame_bytes），6=packet_bounds，7=packet_collect（u64 binding_index、u32 field_offset_bytes），8=packet_advance，9=packet_back_edge，10=packet_tail，11=tile_input_boundary（u32 width），12=tile_input_copy（u64 binding_index、u32 source_offset_bytes、u32 storage_offset_bytes、u32 unit_bytes），13=tile_base（u32 storage_offset_bytes），14=tile_phase_enter、15=tile_phase_advance（均为u64 phase_index），16=tile_private_pointer（u64 phase_index、bool restore），17=tile_private（u64 phase_index、bool restore、bool local、u64 slot_index、u32 storage_offset_bytes、Scalar），18=tile_publish_pointer（u64 tile_index），19=tile_publish_load（u64 tile_index、u32 value_offset_bytes、u64 value_register_index、Scalar），20=tile_publish_shift（同19后加u32 part_index），21=tile_publish_store（同20后加u32 storage_offset_bytes、u32 unit_bytes），22=tile_collective（u64 tile_index、u32 value_offset_bytes、u64 result_register_index、Scalar、TileOp、TileCollectivePart）。无附加字段的part只写类别。查询kind为parallel_range，part使用上述名称，stage_origin使用OperationOrigin形状；局部帧参数不等同全部stack_bytes。范围ABI的FunctionAbi.definition／instance为null，stage_origin不能解释成每条生成控制指令的业务语法或选择条件位置。事实只覆盖实际生成段，内部IR仍归operation_ranges；tile输入快照、阶段控制、私有流以及发布内部生成段已交接，集合核内部五处生成职责也已交接。发布value_offset_bytes相对原Publication记录，IR值身份不是机器寄存器；part_index零起，每段对应原值的四字节片段，storage_offset_bytes为原共享流首lane相对RSP的真实栈位置，unit_bytes为实际写宽。只给真实存储生成tile_publish_store，不按类型补写未使用字段；零长度tile_publish_load没有对应机器指令，实际语法由原operation_ranges交接。原始ABI编码借用原字段逐条写入，与恢复事实共用一张职责标签表，不建立整组FunctionAbi／来源副本。BLCR7与BLCF10共同采用新增标签；旧BLCR1至6、BLCF1至9材料明确不支持，应由匹配生成端重新保存观察材料，查询不重编。TileOp为原IR运算的u32编码：0=ReduceSum、1=ScanExclusiveSum、2=ReduceMin、3=ReduceMax、4=ScanExclusiveMin、5=ScanExclusiveMax、6=ReduceBitAnd、7=ReduceBitOr、8=ReduceBitXor、9=ScanExclusiveBitAnd、10=ScanExclusiveBitOr、11=ScanExclusiveBitXor；JSON tile_operation复用原IR名称。TileCollectivePart为u32子类：0=Setup（依次u32 input_offset_bytes、output_offset_bytes、length_offset_bytes、plane_stride_bytes），1=Identity（u64原始bits，JSON identity_bits为十六进制字符串），2=ScanOutput（无额外字段），3=Accumulate（u32 input_high_delta_bytes、u64 loop_start_byte），4=Fill（u32 length_offset_bytes、u64 loop_start_byte）。JSON part分别为tile_collective_setup、tile_collective_identity、tile_collective_scan_output、tile_collective_accumulate和tile_collective_fill，并共同保留同一tile／原值字节／结果寄存器／Scalar／TileOp。输入、结果和长度偏移相对本函数RSP；循环目标相对本函数字节0，输入高平面偏移仅64位消费。独占扫描只写此前累积结果并推进，再执行当前元素累积；归约则在累积完成后填满当前实际长度。私有结果流每lane写一个u32，64位额外写相隔plane_stride_bytes的高平面，不能把逻辑u8／u16误读成一／二字节私有流写。空结果不补记录，来源与运行完成分别解释。必要执行编码执行映像格式、语言对象对象格式及其机器ABI不变；Rust公开来源职责枚举新增变体按不兼容版本交付。

tile_input_boundary是原块入口对齐与实际长度建立；width是本次原逻辑块宽。tile_input_copy是原单片段的指针准备和有效lane复制循环；source_offset_bytes相对原绑定每条源记录，storage_offset_bytes相对范围入口完成后的RSP，unit_bytes为每lane实际复制的1／2／4字节单元，目标相邻lane按原四字节流步长。64位值按原输入word拆分，不以片段标签冒称完整源类型；多个捕获共享同一原片段时只记录那次实际发射，无消费者片段没有复制范围。实际源stride和有效长度仍由原输入表／长度槽消费，观察不新增布局或运行值。tile_input的Stage上下文与普通parallel_range相同，不伪造捕获语法或B形参。

tile_base是原块基址的实际保存；tile_phase_enter是phase入口的原基址重装，tile_phase_advance覆盖原lane推进、相对块基址的差值、实际长度比较及回边。phase_index从零开始，只解释同一Stage的原计划顺序。tile_private_pointer按restore的true／false分别查询为tile_private_restore_pointer／tile_private_save_pointer；tile_private按方向分别为tile_private_restore／tile_private_save。私有槽JSON的slot_kind为local或register，对应编码bool local的true／false，slot_index为该原IR槽索引，register不是机器GPR，local不是运行地址；Scalar复用前述Print的原u32 Scalar编码，JSON用原Scalar.name的类型名称。storage_offset_bytes是原私有流首lane相对RSP的字节位移，实际地址加4×lane；64位高平面再加width×4，其余类型仍用原u32流，Scalar描述逻辑类型。空槽集合只有原地址准备，不能补造槽记录。原偏移查询一次供机器与事实，阶段、私有流、输入及用户IR范围分别描述，不包住未交接的发布／集合生成段；Stage上下文不等于精确业务源位置，IR槽与phase序号也不是跨版本稳定身份。

下列材料字符串均为u64 UTF-8字节数与原字节，可选值均为u8的0／1标记，序列均先写u64数量，bool为u8的0／1，不直接编码Rust容器内存。Definition依次为module、definition、可选source_path字符串、32字节源文本SHA-256、u64源文本字节数、Range和路径编码。Range依次为u64 UTF-8 start／end、可选start位置、可选end位置；每个位置为u64 line／column，均从1开始，column按Unicode标量计数。路径编码为u32类别及序列，0=Windows原u16 UTF-16单元、1=Unix原u8路径字节，查询保留生成端编码，不从查询机路径重建。材料不包含源文本；缺少定义时不猜行列。

Parameter类别为u32：0=implicit_frame，无附加字段；1=普通／生成形参，随后u64逻辑index、name、instance字符串、generated bool和可选Site。Site依次为Definition、可选实例字符串、u32语法类别（0=statement、1=expression、2=place、3=parameter）、Inline序列、Domain序列和本Site的Range。Inline为可选Definition、可选实例和Range；Domain再于相同三字段后保存target字符串与u32 transition（0=expanded、1=compiled）。两种路径顺序均为outermost_first。未知来源、真实生成形参与隐式Frame继续使用各自原表示，不给无源步骤补造定义；解码只恢复这些只读字段，不构造Source、AST、IR、Frame或资源。

OperationRange按原发射完成顺序保存u64 start、u64排他end、bool includes_nested_operations与OperationOrigin。start≤end≤原.pdata主末端；允许零长度和实际控制父／子范围重叠，不要求按起点排序，不将范围长度求和作为指令数或运行次数。OperationOrigin依次是可选Site与可选GeneratedOrigin，二者可以共同存在，也可都缺失；记录中的未知来源与整个operation_ranges未保留分别表达。选中节的operation_ranges与原NativeCode::code_json字段一致：start_byte、end_byte、byte_length、includes_nested_operations和origin；未提供材料／未保留／手写节为null，已采集空序列为[]，不从机器模式补造来源。

GeneratedOrigin依次为原GenerationReason的现行JSON名称字符串及可选GenerationAnchor。枚举名称仍由原IR唯一提供，字符串只描述该原事实，不用作执行、反射或运行调度。GenerationAnchor用u32类别和按以下顺序串联的字段编码；字段的含义沿原IR来源合同，保持原语法、定义、声明、重写输入与保留事实，不包含原Inst、Reg、Local、FunctionBody、Stage或运行值。Site／Definition与前述同名记录相同；Declaration为Definition形状的声明身份与u32 representation（0=source、1=compiled_interface），JSON使用source_declaration及declaration字段；ContractCondition的span保存带原行列的Range。可选值、列表、字符串、bool和位宽沿本节原规则，Box仅编码被指向的事实，不编码地址。

| u32类别 | 原Anchor | 字段顺序 |
|---|---|---|
| 0 | Syntax | site |
| 1 | CpuTextureSample | sample |
| 2 | Definition | origin,instance |
| 3 | IntegerDivisionPair | quotient,remainder |
| 4 | ExpressionReuse | retained,reused |
| 5 | LocalReadForward | retained,reads |
| 6 | IndexedStoreReadForward | retained,reads |
| 7 | IndexedReadReuse | retained,reused |
| 8 | LocalInputForward | retained,inputs |
| 9 | ReductionInputForward | retained,inputs |
| 10 | EliminatedOperations | retained,operations |
| 11 | EmptyMapRemoval | retained,stages |
| 12 | IntegerDivisionResultPrune | retained,input_results,output_results |
| 13 | ReductionUniformExtract | retained,partition,operation,uniform_operand_count |
| 14 | UnaryConstantFold | expression,operation,scalar,input_bits,result_bits,component_index,component_count |
| 15 | BinaryConstantFold | expression,operation,scalar,result_scalar,left_bits,right_bits,result_bits,left_component_index,left_component_count,right_component_index,right_component_count,component_index,component_count,retained |
| 16 | CastConstantFold | expression,source_scalar,target_scalar,input_bits,result_bits,component_index,component_count |
| 17 | BitcastConstantFold | expression,source_scalar,target_scalar,bits,component_index,component_count |
| 18 | MinmaxKnownValue | expression,operation,scalar,left_ranges,right_ranges,left_constant_present,right_constant_present,same_value,result_bits,basis,left_component_index,left_component_count,right_component_index,right_component_count,component_index,component_count |
| 19 | ComparisonKnownValue | expression,operation,scalar,left_ranges,right_ranges,left_constant_present,right_constant_present,same_value,result_bits,false_possible,true_possible,left_component_index,left_component_count,right_component_index,right_component_count,component_index,component_count,retained |
| 20 | ModuleConstantEvaluation | definition,constant,operations |
| 21 | StageConstantEvaluation | expression,operations |
| 22 | StageIfSelection | condition,value,evaluation,returned,stages,calls,retained |
| 23 | StageForExpansion | statement,counter,first,last,iterations,retained |
| 24 | DomainEntry | context,expansion |
| 25 | StageConstantReference | reference,retained |
| 26 | DomainCallLiteral | call,evaluation |
| 27 | DeclarationReference | declaration,retained |
| 28 | ContractCondition | declaration,span,retained |

ExpressionReuse记录result、paired_input及origin；LocalReadForward记录result_index、result_count、result与origin；IndexedStoreReadForward记录同前三字段及store、read；InputReadForward记录input_index与reads；ReductionInputReadForward记录role、component_index与reads。各index／count为u64，paired_input为bool，origin／store／read为OperationOrigin，reads为相应事实列表。ReuseResult用u32的0=value、1=quotient、2=remainder；ReductionInputRole为0=source、1=left、2=right；IntegerDivisionOp为0=quotient、1=remainder。

EliminatedOperation用u32类别：0=ValueResults（Scalar序列result_types、可选IntegerDivisionOp序列integer_division_results、OperationOrigin）；1=DirectPrivateWrite（Scalar、OperationOrigin）；2=IndexedPrivateWrite（u64 component_count、Scalar index_type、u32 byte_offset、Scalar、OperationOrigin）。StageForBound保存可选Site expression、u32 value及可选GeneratedOrigin evaluation；StageForIteration保存u32 index、bool returned、OperationOrigin列表stages／calls及可选GeneratedOrigin expansion。上述值描述已有编译事实，不是新的执行结果或完成状态。

Anchor中的位模式为u64，component_index／count及uniform_operand_count为u64，已知范围是u64上下界对及相应可选列表，presence／same_value／possible／returned及StageIfSelection的value字段为bool。Scalar沿前述现行编码；UnaryOp用u32的0=FloatNegate、1=SignedNegate、2=UnsignedNegate、3=LogicalNot、4=BitNot、5=Bitcast；ReductionPartition为0=identity、1=project、2=combine；ReductionUniformOperation为0=Constant、1=Uniform、2=BufferStride、3=Unary、4=Binary、5=Cast、6=Math、7=Select、8=ConstantIntegerDivide；IntegerKnownValueBasis为0=singleton_range、1=zero_condition。BinaryConstantFold／MinmaxKnownValue／ComparisonKnownValue的operation及instance／constant／counter字段为原字符串；具体Source与GeneratedOrigin递归关系、字段顺序由同一观察格式交接，不建立另一来源登记表或强制保留已消除代码。

摘要以原长度前缀字段依次消费代码字节、节特征、重定位数量及各项offset／type／原目标名称／带符号节号／value，再消费本节已解析.pdata范围数量及各项start／end；数量、偏移、特征和数值沿原64位字段表示。物理节号在记录中配对，不散列文件位置、符号表索引、私有BLCO／构建记录、SDK或工具链。材料匹配说明同一范围相关的代码／重定位／原声明范围，不证明来源、执行安全或整个包所有内容相同；函数表、展开及其余COFF事实继续读取本次原文件。已有归档成员不因材料存在而被展开。

```text
bc code --object target/module.obj --observations target/module.blcf
bc code --object target/module.obj --observations target/module.blcf --section 1
bc code --object target/module.obj --function exact-symbol --observations target/module.blcf
```

SDK使用`object::code_facts_with_observations(bytes, selection, material)`。先完整核对所有原代码特征节的数量、顺序／物理号、摘要及布局覆盖，再检查原引用目标不超出节长度、完整四字节字段位于节内，以及field+4+原有符号rel32位移=target，再生成选定查询；错误直接失败，不输出部分成功，也不转为无材料查询。成功JSON增加code_observations=matched，原物理节、符号／重定位、文件位置和function_tables保持。每个所选节新增generation，手写类别的reason分别为application_startup、scalar_output、import_thunk；scalar_output还含实际scalar名称。它只描述同节generated_code的原生成职责，范围沿原区域0至节长度，不是原生函数的IR／ABI／源码来源；Function、无材料和非代码节为null。Material每节只交出一次原Layout，区域、引用及此职责直接借用同一值，不另建三份查询状态。选中节增加tail_references；每项为target_offset_bytes及rel32_field_offsets_bytes。null表示材料未交接该事实，包括无材料、三种手写类别和非代码节；[]表示原Function已采集但无引用。代码节regions分别为runtime_function、stack_probe、generated_code、alignment、literal；前三类解码完整原指令，对齐和字面量instructions=null。各分区原bytes合起来重建同一节；非代码节仍保留原查询语义，不据材料伪造分类。没有材料时继续明确保留unclassified；符号选择仍只定位原节，不改变原函数／代码布局。

完整优化的执行对象不携带`.blcf`仍可按原编码链接／运行；保存构建输入的对象与原Object.write对象可复用同一材料，因为相同代码的文件位置／私有记录不成为匹配前置。材料由调用者持有和释放，不延长代码、模块、进程或GPU消费者寿命。最终PE／归档及完整静态运行库、源码来源与实际地址分别沿原.9、.7、.13，不把本接口写成完整原生调试已经交付。


### 原SPIR-V单模块的完整结构查询

```text
bc spirv target/module.spv
```

compiler SDK的`spirv::module_json(words: &[u32]) -> Result<String, spirv::DecodeError>`只借用调用者的原32位words，CLI只读单个小端`.spv`文件。两者不读取B源、不创建Compiler／设备／执行资源，不改写或重新生成模块。查询成功返回独立JSON，临时解析和结果由调用者持有、释放，不延长原GPU消费者寿命；CLI失败返回非零且不输出部分成功JSON。文件尾部不足一个word时给出原字节位置；SDK错误的offset_words是已知头位置或失败指令起点，不能取得时为None，reason交接实际原因，不伪造更细位置。

结果kind为spirv_module、representation为spirv_intermediate、endianness为little，word_count与byte_length是实际长度。header保存全部五个原words及magic／version_word／generator／id_bound／schema；未使用或未验证字段不被默认值替换。instructions按原顺序交接offset_words／offset_bytes／word_count／全部原words、数值opcode和名称、可选result_type_id／result_id；不存在的ID为null，类型指令及其关系继续沿原模块ID解释，不创建另一类型或IR权威。

每个operands条目保存格式kind、原offset_words／word_count／words，以及适用的id、bits、string、value、opcode或symbolic。32／64位字面量用精确十六进制bits保持整数、浮点负零等原位模式，不通过JSON浮点数重算；字符串按原UTF-8解释，原字仍完整保留。entry_points交接原执行模型数值／名称、entry_id、入口名和interface_ids。OpExtInst交接原集合ID、可知集合名、编号及可知扩展名；当前解释GLSL.std.450名称，未取得的名称为null，原编号与操作数仍保留。

实现仅在compiler功能使用optional rspirv的标准格式Parser／Consumer；公开接口不暴露其类型，不调用builder／assembler／optimizer，不替代本项目SPIR-V生成器。运行SDK关闭compiler后不依赖它及其格式库。实际支持由所用语法版本决定，未知opcode／枚举、未支持编码或损坏明确失败，不跳过并拼接成功；结构解码不验证完整shader语义、实际设备能力或执行结果，SPIR-V不等同驱动最终GPU ISA。原StageCode／ProgramImage的完整模块查询及bc build配对导出见下节；单模块解码不额外重建shader集合。

### 原GPU映像全部模块的结构查询

compiler功能提供原`spirv::ShaderCode::code_json()`、`StageCode::code_json()`及`ProgramImage<P>::code_json()`，均返回`Result<String, spirv::DecodeError>`。它们只借用自己的原words／字段，逐模块复用module_json，不读B源、创建Compiler／设备、改写执行代码或重新生成shader；返回JSON及临时解析由调用者持有和释放，不延长原映像与GPU消费者寿命。

ShaderCode结果kind=spirv_shader，保留原ranks布尔角色及module；ranks不是实测数值。StageCode结果kind=spirv_stage，main是原模块结构；tail／scan_add／dispatch／selector为原可选模块或null，finish为原name及module或null，sort为原ShaderCode次序的全部数组或null。不存在的sort与存在的空数组分别交接，不从固定长度截取辅助模块。ProgramImage结果kind=gpu_program_image，program为原程序名，offset_basis=each_module_relative，stages按原次序保留全部阶段。各模块的ID和偏移始终只属于该模块，不按相同ID数值合并跨模块关系。

嵌套失败返回DecodeError，offset_words仍是失败模块内部的原位置，reason包含原stage[index]、字段或sort[index]；不输出缺项或部分成功JSON。查询原StageCode字段不证明它是实际GPU dispatch历史，结构可解析也不证明完整语义／设备可执行。运行角色关闭compiler后不包含这些查询或解码依赖，原BLRI执行编码不增加观察字段。

```text
bc code target/cache/gpu-entry.blbi
bc code --image target/gpu-deployment.blri
```

保存映像的SDK沿原`build::code_facts(path, None)`／`build::image_code_facts(bytes, None)`查询纯GPU。若映像包含CPU代码，提供Some匹配CPU范围材料和原CLI --observations；未提供或错配均明确失败，不伪造CPU分类或空材料。原程序／模板及映像索引、前缀合同、执行代码和资源完成保持，查询不创建驱动或执行资源；结构查询不是不可信映像的执行授权。

`bc build`的每份原`.spv`都由同一shader_file写同名`.spv.json`，JSON就是该份原words的module_json。原program的main、tail、add、dispatch、selection、finish／scatter／root-add及sort全部histogram／scatter／scan／tail／add／gather／key遍历与原文件名保持；本功能不另造模块目录、编译／调度路径或代码权威。先完整解码本模块，再写其.spv及.spv.json；真实I/O失败直接返回，前面已写的文件保持真实状态，逐文件导出没有目录事务、自动回滚或重放。结构输出可由AI独立保存，正式执行不要求携带它；来源映射及驱动最终ISA仍分别沿原.8和.26。

## 显式编号枚举
枚举为普通名义值，必须显式声明已有整数标量 `u8`、`i8`、`u16`、`i16`、`u32`、`i32`、`u64` 或 `i64` 表示及每个成员的编译期编号；声明放在模块接口 `.h`，也可为 `.b` 私有类型：

```c
enum Outcome : u32 { Updated = 7; Unchanged = 9; Alias = Outcome::Updated; };
enum Error : i32 { Denied = -7; Positive = i32(7); };
enum Unnamed : u32 {};
```

成员采用既有模块常量的求值、可见性与循环依赖规则，通过 `Outcome::Updated` 访问；每个成员必须有唯一名称，可以显式共享编号。正整数字面量仍为 u32，负数仍为 i32，不增加枚举专有的隐式转换。`Outcome(123)` 从声明的整数表示构造，`Outcome(existing)` 保持同类型，`u32(outcome)`／`i32(error)` 取回声明的原整数；另一个枚举、不同整数类型、bool、浮点或向量不能直接替代。跨表示转换先显式取回整数，再用已有数值转换。

完整底层整数域均为有效值，未命名编号原样保留；空枚举有效，零初始化存储中的零不要求存在同名成员。它不是只允许列举成员的封闭变体集合，不增加成员查表、运行时标签或未知编号替换。枚举与整数、两个不同枚举即使布局和编号相同也不能隐式互换。

同类型枚举可做六种比较，顺序遵循其有符号／无符号表示；算术、位运算、数值一元操作、数值内置函数、数值类型集合及直接 println 不接受枚举。需要这些操作时先显式取回整数，例如 `println(u32(outcome))`。枚举支持赋值、普通函数参数／返回、位置借用、模块 const、记录字段、内联数组、dataset、tile、view 和开放类型形参；也可加入用户 typeset，参与合法的自定义集合回调。初始化、访问和所有权沿原规则，不因零成本表示跳过检查。

编号枚举直接使用声明整数的大小与对齐：u8／i8为1／1，u16／i16为2／2，u32／i32为4／4，u64／i64为8／8；x64、SPIR-V、算法目标文件和 DLL 沿原标量路径。窄成员须显式形成相应整数，例如 `enum Color:u8 { Red=u8(1); };`，不增加隐式窄化。外部 C 函数按声明的整数传参／返回／位置指针；生成的 C／C++／Rust 字段使用同一宽度与符号的整数，宿主负责接口含义，B 的名义类型不成为运行时包装。带载荷变体仍按下一节的32位判别字及重叠载荷布局交接。

编号本身不定义成功、错误或操作系统进程结果。CPU 应用入口仍只返回 u32／i32，业务枚举必须显式映射；直接返回枚举报 B262。普通值结果、Ok／Err、分支及显式传播见后节，一般带载荷变体见下一节；其他资源载荷及完整失败交接按各节实际交付范围记录。

## 一般带载荷变体
带括号的枚举成员声明一个可构造、可穷尽匹配的变体分支；空括号表示无载荷分支：

```b
enum Message<T> : u32 {
    Empty() = 0;
    Value(T) = 7;
    Failure(u32) = 9;
};

Message<u32> value = Message::Value(7);
match (value) {
    Message::Empty() { }
    Message::Value(number) { println(number); }
    Message::Failure(error) { println(error); }
}
```

每个分支拥有一个载荷类型，记录可组合多个字段。载荷允许普通值、void、固定数组、记录、嵌套变体；普通 CPU 应用的拥有值还可组合 dataset 和拥有聚合。分支名称和编号均唯一，编号使用声明的 `u32` 或 `i32` 编译期表达式；有符号正数按原规则显式写 `i32(...)`。类型／const 参数按已有规则实例化，编号表达式可引用 const 参数；循环依赖报 B160。

构造写作 `Message::Value<u32>(7)`，无载荷写作 `Message::Empty<u32>()`。具有相同完整名义类型的绑定、返回、字段、数组或载荷上下文时可省略类型实参；单独构造不从一个载荷猜测整种泛型类型。类型实参可以是完整的 dataset 包装，泛型替换保留其形状与所有权类型；元素可以持有原规则允许的拥有资源。普通函数的显式调用、显式模块实例导出和类型推导使用同一完整类型，实例身份与其元素类型分别保留。含view元素的完整dataset在显式调用、推导和模块请求中沿同一原类型入口规范化；按值传递、返回及真实view来源继续使用原Data／Proof和资源所有者。普通CPU的read T与inout T可绑定完整dataset，推导和显式完整类型沿原实际位置分别交接只读及独占内容借用，调用后原拥有者继续使用；静态F构造、访问义务与最后消费者的原规则保持。

`match` 的输入求值一次；必须恰好处理声明的全部分支，分支书写顺序不限。模式可以用所属类型限定名，或使用由输入类型确定的分支名称。有载荷分支绑定一个名称，也可用 `_` 丢弃；无载荷分支写 `()`，不能绑定一个不存在的数据值。没有默认分支、原始存储构造或按数值分量访问变体的语法。活动载荷提取时沿当前路径精化其声明类型和各分量的已有数值事实；返回合同中仍成立的输入快照、上界、偏移及布尔条件可继续消费，不将其他分支或改写后的值视为满足同样条件。数值运算、转换及比较先通过 match 取得对应普通载荷；`try` 只接受内置 `result<T,E>`。

变体将32位判别字放在零偏移。整个变体的对齐取4字节与所有载荷对齐中的最大值，各分支共用从该对齐字节位置开始的载荷区。总大小为载荷起点加最大载荷大小，再向上对齐。完整类型及每种载荷布局保留名义身份。重叠载荷中的范围、非零与子资源长度只在对应活动分支内成立；可能存在的其他载荷不为整个值或判别字提供这些事实。浮点载荷按原位交接。内置 result 继续使用原 bool 判别字，Ok 对应零、Err 对应一；普通编号枚举保持前节的整数域与转换规则。

零初始化不是任意变体的默认构造。只有编号零存在且对应载荷递归具有合法零值时，零存储才构成合法值。例如 `enum Only : u32 { Value(u32) = 7; };` 的 dataset 分配后须先写入实际 `Only::Value(...)`，才能读取相关位置。普通记录、数组和 result 的嵌套载荷遵循同一规则。未完整初始化的资源不能通过当前原生按值形参或资源返回合同伪装为已初始化；可通过原 write 借用完整写入。

拥有变体按值移动与返回，match交接活动载荷；重赋值、丢弃与退出沿原所有者释放仍拥有的活动资源。真实GPU完成及download保持原含义。CPU自包含拥有聚合、view载荷、动态拥有位置及条件初始化分别遵守本文对应章节；tile载荷、拥有元素子资源的完整长度关系／批量增长、任意初始化覆盖、其他目标拥有应用、完整资源阶段、异步／远端失败与版本迁移仍待实现。

## 有载荷结果、分支与显式传播
`result<T,E>` 分别声明成功和错误载荷。两种载荷都不拥有资源时，它是普通名义值，支持普通值及`void`，函数值载荷还可包含view；可嵌套结果、枚举、记录和内联数组。无view的普通值可用于模块const及dataset元素；含view的结果沿真实来源在函数参数／返回、普通位置借用、记录／数组和开放泛型间组合，宿主持久dataset中的变体view图按本文末节的实际载荷和资源所有权绑定，源B保持只读消费。拥有 dataset 的结果按下节交接资源，不能按普通值复制。`void` 只在结果／变体分支中表示没有载荷，仍不能声明普通 void 数据；泛型可传递这个静态模型。

```c
enum Error : u32 { Denied = 7; };
result<u32,Error> checked(u32 value) on compute {
    if (value % 2 != 0) { return Err(Error::Denied); }
    return Ok(value + 3);
}
result<u32,Error> next(u32 value) on compute {
    u32 number = try(checked(value));
    return Ok(number + 1);
}
```

`Ok(value)`／`Err(error)` 从已声明的返回、局部类型、赋值位置、具体形参或记录／数组元素类型取得两个结果类型。条件选择的两侧使用同一上下文；没有足够上下文时显式写 `Ok<T,E>(value)` 或 `Err<T,E>(error)`。两种分支的载荷均须匹配声明类型，不因编号相同混用枚举；`result<void,E>` 用 `Ok()`，`result<T,void>` 用 `Err()`。不提供从原始判别字或载荷存储直接构造结果的语言操作。

`match` 求值一次，必须同时有一个 Ok 分支和一个 Err 分支，顺序不限。非 void 分支使用局部名称或 `_` 丢弃模式，void 分支使用空模式（也可 `_`），不能绑定 void 值。模式形状先按结果类型检查，再沿原条件规则分析分支正文；不能从另一个分支读取载荷。

```c
match (next(input)) {
    Ok(value) { println(value); }
    Err(error) { println(u32(error)); }
}
```

模式名称属于该分支，按原局部初始化／赋值、作用域和别名规则使用；允许分支内返回、循环的 break／continue 及资源操作。命名的拥有载荷在分支正常退出时仍由原绑定持有，原静态F、元素清理和资源释放完成后才移出该绑定；实际已移动部分不再释放，提前返回／try沿原作用域出口完成交接。结果本身不公开原始字段，不作为数值或整体比较；取得对应载荷后按其自身规则操作。字段、数组及普通函数组合仍由同一检查器处理，不增加运行时结果对象或动态派发。

`try(expression)` 只在返回 result 的普通函数内显式传播：Ok 交付其载荷，Err 原样交付给当前函数的错误分支，错误类型必须相同；当前函数的成功类型可以不同。void 成功只允许作为无值语句使用。try 不表示异步、事务或自动取消；操作数只求值一次，条件选择和短路表达式仅执行实际分支。没有 try 的普通调用不自动传播结果。

错误返回经过原返回点，本地拥有资源按真实作用域释放，借用仍归调用者。write／inout按声明的完整或条件初始化合同交还输出，条件输出见“条件输出初始化”；Err本身不取消输出义务。已发生的inout／外部效果保持，不自动回滚；GPU工作及存储保持到真实消费者完成。完整失败资源阶段、异步／远端与运行库错误传播仍待实现。

### 表达式求值与提前退出

一元正号`+value`和负号`-value`接收数值标量、数值向量及矩阵，共用同一类型规则；bool、记录、数组、变体、资源和地址／函数／不透明身份不是数值操作数。正号保留实际类型和全部位模式，交回已求值原值，不隐式转换或增加复制；操作数只求值一次。负号沿原位宽、整数回绕和浮点符号位规则；负整数字面量按其最终有符号范围定型。逻辑非`!`接收bool，位非`~`接收整数。普通表达式与函数值声明中的条件共用这些类型规则。

try 可以嵌套在普通 CPU／compute／GPU 函数的运算、数值转换、记录／数组／Result构造、普通或资源实参、位置索引、赋值右侧、条件与循环边界中。操作数和实参仍按原从左到右顺序求值，选择与短路只执行实际分支；try 一旦交付当前函数的Err，当前路径就没有继续值，外层不再读取位置、生成算术、分配资源、求后续实参或执行调用／写入。例如：

```c
result<u32,u32> checked(bool ok) on compute {
    if (ok) { return Ok(3); }
    return Err(9);
}
result<u32,u32> next(bool ok) on compute {
    return Ok(1 + try(checked(ok)));
}
```

普通调用在语句和嵌套表达式中使用同一声明、名称解析与形参求值；拥有记录／Result／dataset沿原拥有交接规则准备，临时拥有值保持到实际调用或当前路径退出。赋值先取得实际左侧位置，再按原规则求值右侧；其中一步退出就没有后续写入。错误返回结束已取消外层表达式的临时位置／实参借用，已产生的临时拥有资源仍沿原作用域释放；真实返回view和write输出在此之前检查，具名借用及初始化义务保持。正常继续路径仍保持位置借用到后续实参或右值完成。外部C函数同样只在全部实参真实完成后调用，错误退出不发生外部写入或虚构write完成。

短路和选择式复用同一分支求值。已退出分支不参与普通值合并，仍继续的两侧必须具有相同类型；真正的void调用仍不能提供数值表达式所需的值。即使条件为常量，未选分支的类型、访问和能力问题仍报告，包括未选表达式中的零除数；未选分支的代码、效果及返回条目不进入实际执行。函数返回值和write输出义务只由保留的真实退出交付，不能让未执行的错误分支污染值范围或要求另一条路径提前写完输出。

错误路径仍遵守声明的write初始化合同，普通Err不自动放弃输出；ensures initialized可按实际结果声明条件范围，见“条件输出初始化”。已发生的inout／外部效果保持。cooperative整块提前返回语义未实现，报B208；结果形式入口、异步／远端及运行库失败仍是缺口。

### 拥有 dataset 的结果

普通 `on cpu` 函数可以声明 `result<dataset<T>,E>`、固定形状 `result<dataset<T,N>,E>`，或让错误分支拥有 dataset；两个分支都可拥有，结果也可嵌套。普通值／void 分支仍遵循前文规则。构造 Ok／Err 时移动活动载荷，原 dataset 或拥有结果不能再读；借用的数据不能转成拥有载荷。

```c
result<dataset<u32>,u32> create(u32 count) on cpu {
    if (count == 0) { return Err(7); }
    return Ok(dataset<u32>(count));
}
result<u32,u32> size(u32 count) on cpu {
    let rows = try(create(count));
    return Ok(rows.length);
}
```

拥有结果可无初值声明、初始化、移动、整份重赋值，作为普通 CPU 值参数或返回交接，也可经开放泛型转交。赋值先求值右侧，再释放左侧仍拥有的旧活动载荷；自移动不会重复释放。match 消费结果，把活动载荷交给该分支的局部绑定；`_` 和未接收的结果沿原 release 释放资源。try 的成功分支移动载荷到表达式，错误分支将其交给原函数返回点；已返回路径不再构造外层结果或求值后续实参。

每条继续路径保有自己的真实所有权。某路径消费结果、另一条继续路径保留时，后者在原合流边释放其活动载荷，合流后源绑定不可再读。作用域、return、break／continue 释放该出口仍拥有的资源；循环回边保持入口的拥有关系与实际存储位置，沿原首轮展开处理重新初始化。一般循环的新载荷分支／存储位置不变式尚未全部推导，不能由未知状态假设 CPU 存储。

资源仍是同一Frame中的原存储，dataset载荷打包原句柄和长度。GPU数据可经结果交接，再由实际拥有者显式download；传递本身不等待、下载或复制。固定拥有数组的动态位置、整份拥有聚合write／inout和CPU自包含拥有view聚合按对应章节规则使用；拥有元素子资源的完整长度关系／批量增长及完整异步交接仍未实现。拥有结果不属于普通C ABI或算法DLL的普通值数据接口。

### 当前原生表示与系统边界

判别字是偏移0处的 B bool（4字节）：零为 Ok，非零为 Err，B 构造为0／1，沿现有 bool 的原生 ABI 读写。对齐为两个载荷自然对齐与4的最大值；重叠载荷从该对齐后的偏移开始，占两个载荷大小的最大值，总大小再按同一对齐补齐。只存一份活动载荷；编译表示按物理u32存储字拆分／重组，u64／i64载荷保留两个字，并按载荷类型恢复浮点原始位、整数、枚举及bool。构造时未用的载荷字归零；对齐间隙不是业务字段，不保证其字节值。

不含拥有资源的结果在 x64、SPIR-V、静态对象和 DLL 共享此布局，C／C++／Rust 记录按生成接口公开 `*_tag` 与 `*_payload_序号` 原始字，保持原 C ABI 借用和同步完成边界。原生声明仍是可信接口，宿主应提供所声明活动载荷的有效值；不做成员查表、未知编号替换或额外 ABI 校验。8字节结果的寄存器传递与80字节结果的间接传递已有 PA3 原生记录，C++／Rust 都可消费算法接口。

本结果表示业务返回，不自动捕获分配、驱动、外部异常或执行框架错误，也不把其伪装成业务 Err。应用入口仍须显式返回 u32／i32，返回 result 的入口适配尚未实现；成功编号、错误上下文及进程退出码分别映射。统一函数、target、独立原生调用及稳定热更新仍按各自长期事项推进。

## 独立 CPU 应用与标准输出
应用入口是 `.h` 公开、`.b` 实现的无参数普通函数，返回 `u32` 或 `i32`，显式声明 `on cpu`：

```c
module applications.start;

u32 main() on cpu {
    if (!println(1 + 2)) { return 1; }
    return 0;
}
```

对应 `.h` 声明 `module applications.start; u32 main() on cpu;`。直接构建与运行：

```powershell
.\target\release\bc.exe -c `
    sample/applications/start.b `
    -o target/applications/start.obj
.\target\release\blink.exe `
    target/applications/start.obj `
    -o target/applications/start.exe
.\target\applications\start.exe
```

正常输出 `3` 和 LF 换行，进程退出码为 `0`。`bc -c <module.b> -o <output.obj> [-I module-root] [--entry main]` 默认以源文件所在目录作为模块根、选择本模块公开的 `main`；显式入口也沿用具体类型／const 参数选择。返回值的原始 32 位作为退出码，`i32(-7)` 保留相同比特。入口参数、其他返回类型或其他执行目标报 B262，私有或不存在的入口沿用 B240。函数体继续按既有类型、返回、初始化和索引规则检查。

### 编译与链接的独立边界

`bc -c <module.b> -o <output.obj> [-I module-root] [--entry main | --program name | --module] [--import module.obj|library.lib] [--instance function<args>]` 只输出 Windows x64 COFF 目标文件；普通应用默认选择 main，`--program` 选择具体算法入口。目标文件保存已经生成的机器码、SPIR-V、符号、标准地址重定位及必要的静态运行库代码，不保存待编译的实现源码或值IR；独立模块携带其公开.h接口。

`blink <input.obj> [module.obj|library.lib ...] -o <output.exe|output.dll>` 链接已编译对象及模块库：普通应用生成 exe，算法单元生成 DLL 和宿主调用接口。链接器和归档器共用 B 的文件读取，按归档签名辨别实际输入。归档成员和从对象恢复的内嵌执行库共享各自的原输入字节范围，直至最后一个消费者结束。链接可以在没有 B 源码、bc、Cargo 或 rustc 的目录执行。含执行框架的目标文件需要 Visual Studio C++ 本机链接工具和 Windows SDK。本次链接释放自己创建的暂存输入，交回准备、链接或释放中最早发生的错误。MSVC 链接失败时交回本机进程的退出状态、目标路径与诊断。链接器从 Windows 安装注册表读取 SDK 根路径，按数字版本选择实际 SDK 库目录，再将目录交给原生链接工具。文件路径沿 Windows 原生表示传递。算法 DLL 的文件主名用于生成 Rust 绑定，必须能表示为 UTF-8。仅含普通值的应用由 Blink 自有 PE 链接器输出。

编译时若需要执行框架，源码 SDK 使用现有 Rust 工具链把框架和 ABI 编译成静态目标代码，写进该目标文件；链接时只解析这些已经编译的代码及本机库依赖。独立原生模块与模块静态库按后节的明确导入和链接方式组合。`bc check` 与阶段代码导出 `bc build` 保留，不承担最终应用链接；旧的 `build-exe/build-dll` 合并命令已移除。

`bool println(value)`接受单个当前整数、f32、f64或bool标量。整数使用十进制，bool 使用 `true／false`；f64使用17位有效数字，f32按C数值区域的 `%.9g` 格式输出，最多 9 位有效十进制数字，小数点为 `.`，按数值使用普通或科学计数法并去除末尾无用零。负零保留符号，无穷和 NaN 使用系统 UCRT 的 `inf／nan` 形式（NaN 可以带符号或分类后缀）；文本不保存 NaN 的负载。末尾附加一个 LF，完整写出返回 true，格式化失败、写入失败或部分写出返回 false，不自动重试。参数只求值一次，不使用返回值也保留这次输出。CPU 普通函数与 CPU 计算函数 均可使用；输出是可观察副作用，不能当作纯数值表达式消除，也不承诺 parallel 参与者的输出顺序。GPU／compute 目标报 B260，参数数量、向量等不支持的类型报 B261。

生成物是 Windows x64 PE32+ 控制台应用，具有原生启动、系统导入、必要的地址重定位、页探测及 x64 栈展开信息；输出使用进程已有标准输出，f32 格式化与超越函数使用系统 UCRT。运行无需 B 源码、编译器、brun 或手写宿主。动态数据与 计算函数 组织见下一节；Windows命令行快照及参数序列已有标准库接口，blink.windows.file::read_binary以Windows x64同步读取交回唯一拥有原始字节，read_utf8复用同一读取／关闭后再严格校验编码；write_bytes消费已有同步句柄的实际写入进展，write_binary创建或截断、create_new_binary排他新建，完整输出后各关闭本次原句柄。rename在唯一编码入口拒绝目标NUL并交回下标，完整UTF-16目标缓冲及原源句柄直接交FileRenameInfoEx，remove_file直接调用FileDispositionInfoEx；本次操作／关闭及最后旧消费者分别交接，原错误不改走另一入口，准确平台支持及错误类型见同一文件库。此前完成前缀、原OS错误、成功零进展与关闭失败的原身份分别交接，不宣称原子发布或失败回滚，见[文件I/O库](../../library/README.md#blinkwindowsfile)；blink.windows.path::try_exists已交接实际链接目标的存在性、真实不存在与原OS查询／关闭错误，见[路径存在性](../../library/README.md#blinkwindowspath)；同模块absolute已交接不要求目标存在的独立拥有UTF-16绝对路径，普通路径沿本次OS规则、verbatim前缀原样保持，见[绝对路径](../../library/README.md#取得绝对路径)；同模块canonicalize沿真实OS目标句柄交回独立拥有UTF-16规范路径，实际链接、扩展命名空间、查询／关闭错误分别交接，见[规范路径身份](../../library/README.md#取得规范路径身份)；with_extension按Windows前缀与文件名语义借用UTF-16输入并交回独立拥有的扩展名替换结果，不要求文件存在，见[扩展名词法替换](../../library/README.md#替换路径扩展名)；join按Windows前缀、根与相对路径规则交回独立拥有的UTF-16组合结果，支持只读参数别名，见[路径组合](../../library/README.md#组合路径)；目录搜索由[blink.windows.directory](../../library/README.md#blinkwindowsdirectory)的FindData、搜索handle和FindFirstFileW／FindNextFileW／FindClose直接交接；调用者先初始化缓冲，仅在成功后解释目录项，立即保存结束／失败的原OS错误，并在最后消费者完成后关闭搜索。编译器CLI／装入器采用、通用字符串、其他输入、目录创建、SDK目录枚举采用、其他平台／架构及源码调试仍待完成。外部 DLL 调用见后节，源码示例见 [独立应用](../../sample/applications/README.md)。

## 应用拥有的动态 dataset 与显式执行
普通 CPU 应用及其普通 CPU 函数可声明本地独占资源：

```c
dataset<Item> rows = dataset<Item>(length);
target(cpu, seed(rows));
upload(rows);
target(gpu, step(rows));
download(rows);
for (row in rows) { println(row.value); }
```

`dataset<T>(length)` 接收 u32 长度并将普通值记录的完整存储初始化为零；长度为零有效。`let rows = dataset<T>(length)` 推导拥有的资源，固定形状可由声明或 `dataset<T,N>(length)` 给出，创建长度须证明等于 N。动态长度不会被用于推导 const 实例。dataset 不能初始化普通值或复制成另一份独占资源；拥有资源的函数返回见下节，不能将其当成普通值返回或存储。普通CPU可由静态F构造含view的dataset元素，实际来源和寿命按[CPU拥有元素的dataset](#cpu-owned-dataset-elements)交接；宿主持久共享图仍由原生资源API绑定。

资源公开只读 `length`。普通CPU函数闭合时，返回数值是否原样来自某个输入，由原Module.units中的FunctionBody／ValueCode、原Call实际参数Proof及局部写集合查询并交回原Returned.proof；原返回模型仍是唯一数值合同。查询沿原函数和真实输出／输入寄存器作归纳，原IR中的所有可能写入与选择共同保持同值时才交接这一关系，导入只消费已编译合同中已有的等值。实际改写、算术或外部结果尚不能确定时保留未知；不枚举递减输入，不以尾调用、never或固定数值替代关系，不保存第二值流图或缓存。资源拥有来源与数值版本分别处理，普通递归输入别名不从ResourceParameter猜长度。resize／take_last交回同一资源的新描述符长度；资源拥有身份相同不意味着新旧数值相等。调用端只消费原Returned.proof已成立的数值关系，不从拥有者另推等长；本数值关系不代表任意递归数值分析已经完成。CPU 下标、字段、固定内联数组、有界游标、ordinal 和初始化规则复用原检查器与访存 IR；普通 CPU 函数的 `read dataset<T>` 共享读取，`inout dataset<T>` 独占读写整份已初始化存储；位置由函数中的真实用途约束，CPU载荷访问需要CPU存储，GPU域需要GPU存储，单独交接句柄或读取length不要求CPU存储；两者都不转移所有权，不能 upload／download 或在退出时释放这份借用。inout 可继续按 inout 转借，或临时转借只读函数，也可调用对应的 CPU 计算函数；参数恢复后原所有者继续持有同一数据。一次调用中，独占的整份数据不能与同一存储的其他整份／元素借用重叠；共享 read 可以重复借用，同一存储中已证明分离的普通元素／字段仍可分别独占。别名按实际资源身份、位置范围和字段投影判断，不按参数名称猜测。应用内合法取得的 CPU 元素可作为现有 C ABI 的 read／write／inout 位置。动态存储不在 计算函数 的逐元素／块内函数中创建。
### CPU 游标的内容事实

顺序for中的元素名始终表示当前元素或其原只读位置，数值控制下标与它分别交接。初始轮展开、continue、break和seek沿原循环状态保留相应身份；初始续边的写入分析仍按完整元素类型投影字段，退出循环后该局部名称结束，可以重新声明。范围for的源变量继续表示真实数值下标。标量字段转借普通read／write／inout函数不把整个元素名称改为标量；实际写范围、初始化与资源生命周期仍按各自原合同检查。

`for (ticket in output)` 绑定当前位置，有可写借用时才能改写；它不是隐式值快照。同一位置尚未改写的标量、记录字段、向量分量及固定数组分量重复读取时，沿同一编译期内容身份传递范围、索引与非零等已有事实。例如两个已初始化的 CPU dataset 可以这样消费：

```c
for (ticket in output) {
    if (ticket < seen.length) { seen[ticket] += 1; }
}
```

直接赋值按写后值更新事实。可能重叠的别名写入会撤回受影响分量的旧事实；通过 `write`／`inout dataset` 调用或可写整域执行交接数据时，撤回其允许写入范围的旧内容事实。资源句柄、长度和显式传输仍由原资源规则负责，不因载荷写入产生第二份身份。只读调用、不相关资源、未写字段和已经取得的独立普通值保持各自关系。

循环及其嵌套迭代按实际写入投影撤回回边旧事实，条件与短路／选择分支沿原合流规则处理。闭合普通B调用的Memory已知实际资源与字段时，只撤回相交共同模型，未写字段和独立资源保持；未知内存仍按真实未知范围撤回。存储Call成功后，resize／take_last保留调用前已记录的资源Borrow，新的根SSA及长度域仍区分存储版本；来源保留不复用旧地址、下标或长度事实。循环长度数值由原NumericEffect交接，自然出口不重新拼入循环体的条件Proof；拥有、驻留、共同元素及prefix仍按实际出口合流。普通while的局部／引用标量先形成涵盖实际回边的头事实，再读取条件及选择未来写；进入嵌套循环前的赋值沿真实语句顺序组合，无写分支保留其入口值，祖先变量使用真实各轮值，不能只取第一次进入外层时的值。条件确能阻止回绕时，嵌套循环保留已知的不增／不减关系；break、seek、条件调用及真实改写继续影响原出口和数值效果，未知情况不伪装成已知。更紧的正文区间只在条件为真后交给正文，不写入自然出口，也不产生运行时clamp或额外资源恢复。当前生产范围为U32归纳消费者，不构成其他数值类型的永久语言边界。公共访问效果中的Resource／Dataset若只有局部元素下标不能公开，按同一已知资源内的未知下标交接可能访问范围，资源根、字段字节、条件和权限保持；该范围不作为精确返回地址或初始化完成证明。未来读取缓存的保守结论不扩大共同模型的实际写范围，write／inout的类型位置、初始化和拥有消费仍各归原合同。此项只修正编译期事实的归属和失效，不隐式复制游标值，不增加运行时索引校验或数据快照。下标实参、字段列、固定数组分量及拥有记录内dataset沿原位置投影交接资源和实际字段字节；分析尚待闭合的循环来源不自动成为任意内存。未知位置仍撤回可能受影响事实，来源快照不证明初始化、不授予权限，也不提前执行下标。闭合普通B调用按同一Memory的实际字段写区撤回非拥有记录事实，write／inout权限不重复充当写范围。dataset参数的物理根、元素布局和字段字节在预分析与真实调用中一致交接，模块出口保留输入位置的真实写区，私有资源不成为公开写；实际按值dataset仍传递Frame句柄与长度；在原完整初始化检查后，实际Data、元素布局及字节位置同时交给原输入Proof，闭合Memory因此按真实资源撤回数值事实。这个位置是编译期存储关系，不把句柄改成原生地址，不增加机器参数、Frame字段、资源副本或释放者。同上下文的地址身份替换保留Local／Buffer的原固定位置；真正重绑的参数、资源、派生、外部及dataset根继续消费原实参映射并撤去旧形参位置。写区和初始化复用真实存储所有者，不跨模块发布局部槽，不改变机器参数、ABI或释放职责。dataset及拥有字段中的实际元素地址沿原InputSlot::Value引用Data.elements的对应分量，与普通值共享原LoopInput；资源句柄由Data槽交接；循环写预分析撤回旧数值事实时保留原LoopInput借用来源，独立拥有Data的循环源沿原Borrow.resources保留带成立条件的物理资源身份，借用Data及普通地址／view源继续交接实际位置与布局，不把元素内容或位置层当作拥有句柄。首次真实游标读取在原数值身份初始化接点同时取得已有共同来源，后续游标赋值保持本次精确来源，不合并已被覆盖的旧地址。循环出口可直接消费已闭合的Value来源；仍待求解的来源和资源存储版本继续原循环图。循环传地址后的真实写入会撤回相交位置的旧数值，写零后不能继续取得非零许可；原循环出口先将每条回边的原值别名交给同一出口值，再合流数值范围与原零次／重复分支；实际改变的值不成为相等身份，延后MemoryValue沿原Clause求解。普通for及其continue保持未写兄弟字段的事实；条件合流只为两侧均存在且不同的值身份建立输出名；缺少既有身份时不提前命名，实际读取沿原位置取得当前共同事实与借用来源，赋值继续交接其真实新值。复合continue保留实际指针写的原存储别名和后续真实读取。循环入口仍由原LoopWrites交接可能写的位置，尚未读取的地址不能由语法名称推断为不相交。间接影响的本地数值由原循环输入槽命名，入口与真实回边仍由同一LoopState持有，沿原Memory与这些生产者求解当前事实；同一数值效果消费本地存储分量与拥有dataset的长度赋值；resize及dataset构造的确定长度参数沿原独立赋值、分支和真实继续边合流，只保留入口与回边共同保证的数值界，数值界不恢复旧资源身份或冻结为类型count。对同一u32存储位置及循环中不变的常量，`x=c-x`沿原整数运算按回绕语义形成覆盖入口值与反向值的循环范围，原下标和非零消费者直接使用。同一反向与不变路径的顺序、分支及继续边沿同一数值效果合流。dataset绑定的元数据写直接交给原LoopWrites本地槽集合；自然出口按同一写槽选择实际重绑定的dataset，沿原LoopState把入口与真实继续边的长度和共同元素模型交给原Proof及Returned模型；根借用保留原LoopInput的来源，数值模型交接不再次合并根条件。元素归属消费原DataReference交接的实际根；仅有一个来源且就是当前Data根时，直接取得已确定的完整根作用域，多来源、别根和真实嵌套按各自实际关系处理。原Edges保留同一输入边的完整正负Condition；同来源union直接沿原Condition::logical(any=true)合流两根，空来源持有原完整条件，不先拆正根再重建负支；真正Clause入口insert_when继续其原职责。来源边输出名字绑定只交付正条件时，先取原正Clause，再由既有Clause::output_bound换名，不为未交付的外层负根建立映射。正根Branch内部的原Condition两支继续由同一换名所有者交接，最终边的完整条件仍在原insert_when形成；普通Condition、真实Proof及调用替换继续各自实际消费者，不假定两根互补。编译期未解析来源的只读判定沿原sources递归携带同一Clause路径合取，不先生成无人接收的sources／inputs／calls／reads限制借用和guard负根；Contents仍沿原局部来源判定及resources投影独立查询，真实限制借用、未知／不可能、内部Condition两支及最终B271职责保持。局部存储逃逸的只读判定同样沿原来源及Clause路径直接查询；Derived／Dataset只交接原父借用与路径，Local／Buffer及其他终态仍由原Origin局部性分类负责，Resource／External不因持有借用就被改判为局部。查询不先复制Origin或形成随后只用于bool的四类限制借用；实际位置、共享布局与来源、权限、初始化和B227拒绝保持。元素归属与循环写影响先合取同名LoopInput的两侧条件，结果完整等于常量true或false时直接消费该关系，只有非确定关系才查询本次path。正根仍有前提而负根Impossible不能当作恒真。普通正分量消费者继续取得原正Clause，原负分量消费者直接使用已持有的负根。selected／root的完整性、Source／Call关系、实际下标及字节范围仍归原消费者，不由确定输入关系授予资源权限。循环数值保持分析只为实际U32归纳消费者生产头证明；其他分量继续交接原初值及零次恢复，真实读取／投影在原绑定处取得其需要的输入名称与合同，不依赖未使用头证明的重复建模。返回元素检查在原元素作用域投影已有拥有集合，不重复并入已由输出汇总或父层交入的当前根。闭合普通CPU及compute值调用复用原Module所持同一实际实例合同，逐项交接真实写访问自身条件、Access与written Proof；compute可用目标声明不把已闭合调用改成未知外部写。原合同的CPU兼容分析不选择本次执行设备，实际调用继续原native／inline及CPU／GPU路径；没有对应闭合合同的外部、间接及GPU专用效果仍由原所有者解释，不按函数外形假定纯。写预分析按当前路径逐项生成原Write::Memory，不先形成无人消费的总Borrow；真实调用按原顺序归并写来源后统一限制本次调用路径，保留MemoryTarget::Call的必要总写关系。真实写值消费前，各条访问仍按同一路径限制；原write_address_values直接借用调用点已有的总写Borrow，只映射当前初始化，不重新归并来源。逐条MemoryEffect继续解释实际字节与written Proof；未知、已知空写、pending、快照与存储版本按原语义处理。拥有状态、实际驻留及长度的uniform／constant事实仍按原数据职责合流，零次执行和break沿原条件处理，普通完整遍历继续由原完成生产者交付。非零使用消费闭合后的实际事实，写零撤回旧非零，未写分量保留当前不变量；break退出不作为下一轮生产者。条件沿现有规范编码比较，不因等价DAG的形状制造数值变更。完整借用与内存安全目标及剩余缺口仍见Issues C-036.5，不将局部能力视作完整保证。循环内创建／resize后按值移交普通拥有dataset的原消费者保持独立资源数值事实，具体已完成范围见FixedIssues C-021.1.4.5.4.3.2.5.2；真正未知效果仍按原Memory处理，不能仅凭拥有参数省略可能写入。来源快照不证明初始化，真实读输入只使用原初始化职责取得的事实；原子读写继续其实际权限与内存序。普通闭合调用的原Memory循环消费已完成，具体范围见FixedIssues；普通条件调用的数值输入／返回关系仍按Issues C-021.1.4.5.4.2.2推进。
### ordinal 的位置区间与不重叠借用

CPU dataset 游标的 `ordinal` 条件会精化当前实际位置的索引区间。例如 `ordinal(ticket) > 0` 成立时，`ticket` 与同一 dataset 的第零条记录不重叠。位置仍属于原资源，数据内容所指的其他下标仍须单独证明；位置有界不自动证明 `ticket.index` 等载荷有界。

原数值条件和等价来源交付位置上下界；分支合流取各实际继续路径位置区间的并集。仅在某一路径成立的窄区间不能泄漏到分支外，短路和选择同样遵守这一点。循环写入分析沿相同区间排除已证明不重叠的常量索引；索引变量若在本轮写集中被改写，即使后来恢复入口值，也不能据入口常量忽略它的实际写入。

两个位置参数的独占性按位置区间与真实分量判断，同一资源内的合法不重叠位置／字段可以一起借用。完整资源或拥有聚合与其内部位置仍按整体拥有关系检查，不能因位置区间精化而同时转移或独占借用同一存储。以上均为原编译期位置与事实的交接，不增加运行时检查或复制。

当前整份可写记录的普通原生调用之后，某些调用前的字段索引关系仍会被撤回；单独字段借用与完整记录效果的精化范围有区别，完整字段输出契约继续研发。

`upload(rows)` 将拥有的 CPU 存储交给同一应用的 GPU 设备，`download(rows)` 等待原生完成点后交回 CPU 存储。两者是语句，保持资源身份、容量、已拥有的有效域及逻辑载荷值；传输本身保留已证明的数值事实，实际计算和写入仍撤回受影响的旧事实。没有第二份可写数据。方向、所有者与当前位置由前端检查。条件合流后要继续访问，相关分支须给出一致的位置；循环继续边须沿原拥有树归还入口及其活动子资源相同的位置；Data根、元素子资源与普通整份借用使用同一原位置查询。read 借用不能转移存储。CPU 对 GPU 数据的索引或遍历不会隐式下载。

CPU应用的整份inout dataset借用沿普通原生调用交接原Frame的句柄和长度，数据访问仍由实际存储所有者执行，不复制数据，也不增加锁或调度器。固定形状、const 推导、nonempty／index 输入约束沿用同一资源长度与身份。并行实例只拥有当前独占元素，不能把它提升为整份 inout dataset；可继续按已有 write／inout T 规则转借当前元素。普通 CPU write dataset 借用的完整初始化义务见下节；它不能因已有物理零值而取得旧值读取权限。正式示例见 `sample/applications/borrow.b`。完整dataset绑定泛型inout T时同样适用：名称、拥有字段、实际字段投影及嵌套位置只求值一次，保留真实元素类型、固定形状和输入初始化。临时dataset在原调用scope持有唯一拥有者，借用不能逃出其实际寿命。循环写入预分析区分dataset内容写和拥有位置替换，不因未实例化的语法T撤回仍真实有效的长度；内容修改和普通变量写入仍使相应旧事实失效。循环内泛型调用可从own(value,F)推导完整owned<T,F>；预分析与实际构造使用同一静态F及载荷类型，不提前求值或转移拥有权。

`target(cpu, name(args))` 与 `target(gpu, name(args))` 选择实际执行位置，入口可用原具体类型／const 语法。参数借用应用资源并按原 uniform 类型传值；类型、固定形状、目标与独占别名仍受原规则约束。同一资源在一次 计算函数 调用中只绑定一次。CPU 与 GPU 同步调用都等待本次真实执行完成；GPU 结果仍留在原 GPU 存储。CPU 消费通过显式 download 取得结果，计算完成不等于传输完成。原生 SDK 的异步提交接口继续保留自己的完成对象。prefix 关系沿同一所有者、实际参数重排及显式传输保留，CPU 写借用计数会撤回旧关系。

编译器在构建时生成 main 机器码及全部被调用 计算函数 的原生映像。优化掉的调用不生产对应映像或运行库依赖；运行时仅在实际首次调用时装载范围函数／GPU 管线，实际上传或 GPU 执行时才创建设备。应用帧只绑定资源变量，记录、传输、执行和完成后的释放继续由原 Dataset、DomainBindings 与设备所有者负责。正常块结束、return、break、continue 和 seek 释放相应局部资源；最后的 GPU 消费者结束前不释放物理存储。

带这些资源的Windows x64应用将同次B代码、框架静态目标和资源绑定一次链接到同一exe；源码SDK只在编译阶段使用Rust工具链生成框架与ABI的静态目标代码，blink只链接这些已编译产物，B算法不转译为Rust。部署复制exe及用户声明的外部DLL，使用 GPU 时具备系统驱动；运行不携带编译器、SDK、B 源码或 brun。运行库错误输出真实原因并以非零码结束。完整 CPU→GPU→CPU 示例见 `sample/applications/datasets.b`；动态部分移动的拥有数组／动态容器与view图、其他资源种类返回、一般动态子域／数组初始化推导、其他执行域的整份独占资源借用、GPU view／纹理／AS 的应用构造及通用标准库继续开发。

### CPU 函数的整份 write dataset 输出

普通 `on cpu` 函数可声明 `write dataset<T[,N]> output`。它独占借用已有存储，位置由实际CPU访问或计算目标约束，不取得所有权，不能重绑定、移动、返回该资源或转移 CPU／GPU 位置。调用方即使传入已清零或已写过的资源，函数也不能读取该 write 借用的旧内容；只读借用不能提升为 write。每条正常返回边必须完整初始化全部元素的逻辑字段，零长度域无需实际写入。接口与实现、公开模块及已有类型／const 泛型沿相同访问契约检查。

```c
void prepare(write dataset<Entry> rows) on cpu {
    for (row in rows) {
        row.value = ordinal(row);
        row.tag = ordinal(row) ^ 305419896;
    }
}
```

完整 `for (row in output)` 或已证明从零／已初始化前缀到该资源完整 length 的数值 for，可把每条真实继续边已写入的字段交给全域；不同字段可以分次完整遍历。已知具体下标的字段写入也可共同覆盖固定形状。break、continue、seek 和提前 return 保留真实退出：跳过元素或字段不会凭循环结束获得初始化，后续完整写入仍可完成义务。空域事实随实际分支、短路、选择式和循环出口交接；不能因另一条路径的写入而读取旧值。

写入后的具体字段可以读取或按 inout 转借；完整资源初始化后才可作为整份 read／inout。嵌套 write dataset 使用自己的输出作用域，完成后交回调用方的初始化责任；标量／数组 write 形参和可信 C ABI write 同样只交付声明的实际位置。数组动态下标的写入不读取旧内容，也不据一次选中位置的写入声称整个数组已初始化；动态读取仍须证明所需位置已初始化。

`target(cpu, ...)` 消费已编译 计算函数 的实际域与操作：无 prefix／where 的完整 map、同长度完整扫描／排序可以交付全域；归约写入实际输出元素与字段，compact／分段归约交付实际计数字段。记录中的其他字段不会自动初始化。有效前缀、稀疏选择和容量尾部有区别，不把 write 参数、实际物理零值或一次 计算函数 成功当成全容量写入。

写前读取／inout 报 B142，返回缺少完整输出报 B229，重叠借用、所有权逃逸和重绑定沿 B227 拒绝。初始化属于现有编译期 Position／initialized，运行时没有初始化位、额外清零、存储副本或新 ABI。当前完整遍历、具体下标及已知域 计算函数 输出已有实现；一般 while／seek 归纳、动态内联数组的逐位置覆盖、任意动态前缀与容量余部的初始化组合仍待完善，属于能力缺口，不是这些程序在语言方向上被禁止。正式百万记录消费者见 `sample/applications/borrow.b`。

### write数据集的新内容与整域组合

write描述借用进入时不消费旧内容，不等于永远不能读取该存储后来产生的新内容。沿原初始化事实，实际已写字段可读取；整份read／inout转借须具备本次作用域内完整域与全部逻辑字段的初始化。CPU普通函数既有实现保持；compute／GPU阶段及无源码对象／归档现已使用同一初始化关系，并完成Release27和实际CPU／GPU执行，具体范围与交付见FixedIssues C-029。

子write借用不继承其输入旧值的读取能力，read借用不获得写权限。有效前缀、稀疏选择、不等容量、未激活载荷及不完整字段须保持自己的实际范围，不能被当作完整输出。普通CPU输出义务与整域显式前缀效果各按原契约记录；不取消合法部分域，不因完整读取尚未成立而补清零或改写用户算法。

外部旧值读取义务、实际阶段reads／writes／跨元素依赖与真实完成分别表达；新内容的读取必须来自先前实际生产及对应完成关系。同一parallel阶段跨元素读写仍遵守原冲突规则，不能以源码语句先后冒充设备全域完成。初始化在原Position仍可用的生产点交给原Output，原函数类型作用域内逐字段累积，全部字段和完整域成立后原Root可供read／inout调用；clear_prefix只交付实际计数字段，集合操作和无源码模块消费同一完成效果。删除每个parallel必须立即填满全部字段和出口按总write猜全记录的旧逻辑，不新增运行时标志、初始化表、存储副本、第二Frame或隐藏同步。
### CPU 应用函数返回拥有的 dataset

普通 `on cpu` 函数可声明 `dataset<T>` 或 `dataset<T,N>` 返回，交付函数自己创建并拥有的存储；拥有元素构造按[CPU拥有元素的dataset](#cpu-owned-dataset-elements)使用。例如函数 `.h` 声明相同签名，`.b` 实现：

```c
dataset<Item> create_input(u32 length) on cpu {
    dataset<Item> input = dataset<Item>(length);
    target(cpu, seed(input));
    return input;
}
```

调用方使用 `let input = create_input(length);` 或匹配的 dataset 声明接收资源；`return dataset<Item>(length);` 和返回另一资源函数的结果也成立。资源不会复制，返回后的释放责任属于接收该结果的作用域。函数先清理其他本地资源，保留所返回资源；嵌套调用、分支和循环提前返回使用同一规则。`read`／`inout dataset` 参数只是借用，返回它们作为拥有资源报 B227。资源结果不能传给普通值参数或初始化普通值，类型不符沿原 B124／B224 处理。

返回声明的元素类型必须相同，固定形状需要证明实际长度等于 N，否则报 B167。类型与 const 参数继续沿既有专门化选择；普通返回不从动态长度反推 const。不同返回点分别保留实际句柄与长度，后续读取 `length` 得到所选资源的真实长度。

函数的 CPU 执行位置与资源的存储位置分别处理：函数可以返回经显式 upload 的资源，调用方继续 target(gpu, ...) 或 download，实际完成点与有效域关系留在原资源所有者。多返回位置的存储位置不一致时，其合流位置未知；需要确定位置的访问或传输沿原 B266 规则拒绝，不自动迁移。普通 CPU 计算函数 的并行实例不能据此创建动态 dataset；应用帧之外的资源构造／调用仍报 B266。

单独调用资源函数而不接收结果时，在该表达式结束后释放结果，GPU存储保持到最后真实消费者完成。CPU资源结果通过普通原生调用和有限独立模块交接同一Frame的句柄／长度，不是可复制普通值或公共C ABI句柄。拥有返回的子资源保留原拥有者的实际来源与选支关系，返回值的输出分量身份不改变该资源来源。拥有记录、固定数组动态移动／补回及条件资源输出见对应章节；完整资源阶段、拥有元素子资源的完整长度关系／批量增长和其他资源种类继续开放。正式示例见sample/applications/datasets.b。

不同动态元素的内部view借用沿实际已求值下标、资源字段及真实父来源区分。借用某元素的子资源后，已证不同元素的子资源可独立替换；替换仍被同一view消费的资源或提前释放父拥有者仍须拒绝。未知选择继续按全部可能来源检查，共同元素模型不表示所有实际元素是同一资源。原参数／对象合同保存共同输入族的实际选择，调用点只实例化借用来源，保持源快照、普通数值关系及最后消费者的原职责。



### CPU dataset 的局部移动与拥有形参

普通 CPU 应用中的 `let target = source;` 或匹配的 `dataset<T[,N]> target = source;` 将拥有资源移到新局部绑定。原句柄、实际长度、固定形状、CPU／GPU 存储位置与有效域保持；没有载荷复制。移动后 source 不能再读取 length、索引、传输、借用或再次移动。已有普通值赋值规则保持；资源重赋值与移动后的重新初始化见下一节。

普通 `on cpu` 函数中，未写 read／write／inout 的 `dataset<T[,N]> parameter` 是拥有形参。调用从左到右求值实参，并在对应实参处交接唯一所有权；允许传入拥有资源的名称、构造或资源函数结果。拥有形参不因CPU控制执行而强制CPU存储，元素类型与固定形状仍须匹配；实际读取、计算目标及显式传输提出静态位置条件，纯拥有交接保留实参位置。函数能读写、转交、显式上传并返回这份资源，或者在结束时释放它；调用方的原绑定随交接失效。类型与 const 泛型、公开模块接口沿同一规则推导和绑定。

```c
dataset<Item> create_input(dataset<Item> storage) on cpu {
    let input = storage;
    target(cpu, seed(input));
    return input;
}
// main 中：
let allocated = dataset<Item>(length);
let input = create_input(allocated);
```

read／write／inout dataset 仍只是借用，不能转成拥有形参、移动到局部或作为拥有资源返回。一次调用中的拥有交接不能与同一资源的其他拥有或借用实参重叠。后续实参中的嵌套调用若消费了此前捕获的整份／元素借用，调用同样不合法；没有运行时别名补救。`take(rows,rows.length)` 在第一个实参移动后也不能再读取 rows；可以先在普通局部保存长度快照。元素游标仍借用其所属存储，游标有效时不能消费整份 owner。

if、短路与选择式只合流实际继续路径：一个继续分支已消费某绑定，另一个分支在合流前释放自己仍持有的那份资源，合流后原绑定不可用；已 return 的路径不影响其余继续路径。循环回边须保有入口各绑定的所有权与物理位置，不能在后续迭代重复消费同一入口资源。循环内新建资源可移动、消费并在 continue／seek 前清理；消费入口资源后 break／return 的路径可以退出。break 与条件失败的所有权差异直接在各真实退出边释放，不增加运行时拥有标记。所有释放仍由同一 Frame 与最后真实 CPU／GPU 消费者落实。

移动后访问、借用逃逸、重叠交接和不合法循环回边报B227；位置不符沿B266拒绝，元素与形状沿B124／B167。普通值dataset、固定拥有数组动态位置及条件资源关系采用对应章节的实现；拥有元素子资源的完整长度关系／批量增长、其他资源种类、其他目标普通调用及完整内存模型仍待完成。

### CPU dataset 的重赋值与确定初始化

拥有资源的普通 CPU 局部和拥有形参可以用 `=` 接收另一份拥有资源。元素类型与绑定声明的固定形状保持；没有显式固定形状的绑定可以接收不同长度。无初值的 `dataset<T[,N]> rows;` 只声明绑定，不分配存储、填零或提供可读句柄；每条继续使用它的路径都须先交付实际资源。

```c
dataset<Item> rows;
rows = create_input(dataset<Item>(0), length);
let previous = rows;
rows = dataset<Item>(length); // 原绑定重新取得另一资源；previous 仍拥有先前存储
rows = rows;                 // 同一资源移出再交回，没有释放或载荷复制
```

赋值先从左到右完成右侧的真实求值与所有权交接，再结束目标仍拥有的旧资源，并把新句柄及长度交给目标。右侧已移动目标时，不重复释放原存储。被覆盖的 GPU 存储仍由原设备所有者保持到最后真实消费者结束；新资源的 CPU／GPU 位置及有效域来自实际右侧资源，不从旧绑定继承。read／write／inout dataset 只是借用，不能重绑定；元素或游标仍在借用某存储时，不能覆盖或移动该存储的 owner。

分支只合流实际继续路径。只有部分继续路径初始化或持有某绑定时，合流后该绑定不可用，仍拥有的一侧在自己的退出边释放；后续可以重新赋值取得资源。循环允许在真实首轮初始化资源，再由后续迭代沿同一拥有关系覆盖或交接；必须排除未初始化的条件失败／break 路径才能在循环后使用。循环继续边沿原拥有树满足后续迭代入口及其活动子资源的所有权与 CPU／GPU 位置，不能重复消费尚未补回的入口资源。初始化义务来自实际读取所需的字段和下标，沿真实入口与继续边的资源版本交接；当前迭代已写入的范围可供读取，只替换而不读取内容的资源不要求完整初始化。原 for／while 条件及实际下一游标决定是否还会进入读取路径，break、return 和到达终点的 seek 不产生下一轮读取义务；一般循环不变式仍须具备当前可证明关系。continue、seek、break 和 return 保留实际执行顺序与清理责任。

长度、下标和缓存谓词是针对实际值及资源来源的事实。资源被替换后，旧资源的 length／索引域不会自动成为新资源的范围；相同存储的移动仍保留来源，实际数值关系可以继续沿原 Proof 证明。绑定本身的位置不作为永久资源身份。未初始化／已移动访问、借用覆盖与非法回边报 B227，元素／形状和 CPU／GPU 位置分别沿原 B124／B167／B266 规则处理。

局部槽、Proof、控制流和应用Frame共同交接资源绑定，不增加运行时拥有标记或资源镜像。固定拥有数组的动态部分移动、可证明初始化及CPU自包含拥有view聚合按相应章节使用；拥有元素子资源的完整长度关系／批量增长、任意初始化覆盖、异步／远端借用和其他资源种类仍待完成。循环数值分析的当前范围见“循环整数位移、赋值与有限反向不变量”，不等于一般循环不变式求解。正式流程见sample/applications/datasets.b。

### CPU 应用中的拥有资源记录

普通记录可声明 `dataset<T[,N]>` 字段、拥有资源的 result 字段及嵌套拥有记录；含这类字段的记录是拥有值，只能在普通 CPU 应用的资源组织中构造及交接。接口 `.h` 与实现 `.b` 使用相同记录和签名。例如：

```c
struct Work { dataset<u32> input; dataset<u32> output; u32 epoch; };
Work create(u32 n) on cpu {
    return Work(dataset<u32>(n), dataset<u32>(n), 3);
}
// CPU main 中：
let work = create(1000000);
let input = work.input;
work.input = dataset<u32>(1000000);
work.epoch = 7;
let next = work;
println(work.epoch); // 普通字段仍可复制，资源叶已交给next
```

构造按声明顺序求值，每个字段必须匹配自己的完整类型；dataset 保留实际形状、CPU／GPU 位置和原 Frame 资源。拥有记录可无初值声明、经局部初始化、拥有形参及返回移动，可与 result 相互嵌套，也可通过开放普通类型形参转交；`struct Work<T>{ dataset<T> rows; u32 epoch; };` 按原名义记录具体实例化。通过dataset形成的有限递归拥有树沿同一资源树构造、借用、移动、返回及释放，元素类型按需解析，编译期不预先构造无限层拥有值。实际子资源及其CPU／GPU位置继续沿当前路径交接；这不授予共享可变图、循环拥有或跨机地址语义。

读取或移动一个资源字段不影响兄弟字段；普通字段仍按原值语义复制。移动整份记录需要全部资源叶仍拥有、全部所需普通位置已初始化；移动后只有资源叶失效，已初始化普通字段仍可读取、修改及借用。补回资源字段后可以再次交接整份记录，普通字段无需因资源移动而重复写入。字段／整份赋值先求值右侧，再释放左侧仍拥有的旧资源叶；部分移动、自移动、覆盖及未初始化普通字段都沿真实消费关系处理。

dataset 字段可用 length、CPU 索引、元素游标、显式 upload／download 和真实 计算函数 调用。for遍历拥有记录元素时，游标的子dataset字段可直接用于只读元素遍历或read dataset调用；它们借用原记录中的实际资源，不复制或移动拥有值；单独字段可沿原 read／write／inout dataset 规则借用，普通字段可用普通位置借用和原 C ABI。固定数组的普通值字段沿原动态选中位置及完整 for 初始化规则；字段的实际地址、偏移、对齐及填充保持。相同实际资源的拥有交接与元素／整份借用不能在同一次调用重叠；游标有效期间不能移动或覆盖对应资源叶。普通字段位置与记录值的普通字段副本分别处理，不增加整记录互斥。

嵌套CPU游标保留原Data及Proof所持本次捕获资源的实际Borrow与父Element路径，现有当前循环命名来源沿同一集合交接；命名列表不能替代完整派生来源，空命名列表不表示没有资源。索引、字段、真实写入及最后消费者仍由原所有者解释，不增加运行期模型或检查。循环外已经成立的子dataset长度，可在真实地址／选择保持、且内层循环只改其载荷时交给正文使用。父记录描述和独占子资源载荷保持各自真实物理存储；实际描述改写仍撤回旧长度。元数据沿已发射读取交接；未定子下标的写按真实已知Frame的严格拥有后代范围解释，父及祖先描述不受该后代写覆盖，实际描述改写与未知写仍撤回旧长度。不制造句柄、复制资源或隐式传输；不同动态元素的写入选择沿实际数值条件保留，整数字段的完整字节非零读取消费已有组件条件；同一或可能重叠位置写零后不能继续取得非零许可。未来直接写的分支采用条件能从稳定输入投影时，沿既有条件共同参与元素选择与完整覆盖，未采用写不撤回入口数值事实；可变或未知条件仍按真实可能写处理，不从一次入口值推断跨轮不变。数值归纳在原Scope已提供稳定条件时，沿当前If的真／假分支消费同一关系；外层已成立条件下的修复不混入该路径不可能执行的分支。稳定bool、原复制快照及已有u32等式关系可据此保留合法前提；真实可变guard、引用／外部写或continue跳过修复仍按实际效果处理。原Scope对稳定u32的<、<=、>、>=沿同一input_snapshot交接真实可选Operand及Proof，由原Condition分别形成真假case的必要事实。值间关系只由真实已命名两侧进入原Below及constrain_proofs，真实常量继续使用原Minimum／Maximum；只有一侧具备数值身份时，已命名侧直接消费另一侧原Proof的类型／数值范围，未知值间关系保持未知，不伪造寄存器、值或常量。真假case各自交接必要界，不能以单侧必要界互求补集；没有完整操作数的相等比较保持未知，不创建第二数值模型。数值If继续消费同一Clause；真正可变或未知来源仍保留可能分支。原顺序事实的矛盾查询直接消费实际方向与严格性：相反方向至少一侧严格时互斥，两侧均非严格仍允许相等；同一关系在原contradictory与cross_conflict处理，不向生产者补写冗余弱关系，也不新增求解器或值模型。原Proof的分支条件合流同时消费本分量已有Condition与nonzero事实：已有条件及其关系原样交接；没有Condition且该分量已确定非零时，以确定真条件参与同一分支合流；其余保持未知。两侧都没有条件时沿原共有数值事实合流，不额外形成条件图。实际写入继续由原所有者失效，不从入口或header恢复旧事实；循环未进入条件写分支时，原非零值的关系可继续沿返回合同交给实际调用者。未来写查询中的dataset.length按原描述符的长度分量取得只读u32数值快照，保留原Proof的已知或未知关系，不把内建元数据送入普通字段查找；它不提供可写位置，也不从元素共同模型补造具体读取的数值身份。原数值Scope只借用本轮已计算的LoopWrites；已有局部拥有者按实际槽保留未写分量的原Proof，已写分量、未提供未来写范围、非本地位置或未知外部写继续未知。普通不可变Value仍沿原数值事实交接，比较入口不再按整个根的可写标记拒绝，Bool与其他分量复用同一快照职责；不从旧Data.length或共同元素模型恢复数值，不生成额外读取、身份或状态。原数值If沿同一次LoopPlace::Element目标和当前Clause解释资源字段条件：实际容器、父选择及字段相对本目标的元素或祖先域确定，且原未来写不改其地址和真实字段字节时，使用已有ElementComponent／ElementParent形式名字；值和范围仍未知，该名字只在本次目标域内存活，不是共同DatasetContent的具体读取，也不生成Reg或运行状态。原资源域查询与真实Frame字节重叠由同一所有者复用；未知别名、地址或相关字节被写、外部写及无法确定的选择保持未知，不因Reg编号不同推断无别名。闭合普通调用的可写Memory沿原Origin的资源、元素下标、layout／words与Scalar字节，经原拥有者交接实际Frame或严格后代，与直接Position共用原存储投影；常量下标及声明布局保留字段字节，动态下标或未知布局仍是整个已知Frame，归属不明仍未知。逻辑位置的字节关系与源／寿命是否闭合分别消费：有实际下标的Resource／Dataset与被查询位置只含同一个原LoopInput名、无源或调用边时，原MemoryRegion使用相对字节判断实际覆盖，不再因整体pending把兄弟字段一并视为写入；不同输入、混合来源、未决调用、无下标view及外部未知地址保持原未知关系，生命周期与可用性检查保持原职责。范围for的原start／end输入在未来直接字段写摘要中交出原元素选择条件：不可写游标直接作为dataset下标时，原loop_write_projection沿同一次位置递归携带既有Condition，嵌套dataset按真实目标元素域推进，数组槽不增加元素域；原Write::Position.when由共同元素更新直接消费，未被该范围选中的父或叶元素保留原事实。范围界限使用原稳定输入快照，seek按原语义撤回下界；可变或未知界限、表达式下标、调用Memory与while局部仍按真实已知关系处理，不把入口初值当作全部迭代。该选择只在原编译分析期间存活，不改变对象编码、资源ABI或运行状态；变化父while的关联计数与额外父清理仍见Issues C-020.1.3.2.1.2.2.13.4.1.10.1。原数值归纳在同一次LoopContext查询内消费各候选已经交出的Shift、入口与真实guard必要范围；每轮确定向guard外推进且原前缀不发生u32回绕时，原摘要给出共同迭代上界，再约束同一拥有者的Proof。正文进入值与条件头／退出值分别按实际迭代位置交接，不把正文最后一次进入前的上界施加到退出后；break及真实回边继续由原循环状态处理。共同次数为零时，原while空循环出口保留条件本身的求值与副作用，正文不形成执行；不把未知写、无进展或可回绕的循环猜成有限循环。沿原候选摘要一次计算，不增加数值身份、持久计数器、Scope字段、图表、回边状态、AST遍历或第二模型，资源／字节／ABI保持原职责。未来while直接条件同样沿原数值所有者交接：Scope在本次while的原写查询中只收集一次Write，以同一临时集合交接；原numeric_loop_paths在已有表达式效果、分支、continue及真实嵌套影响中，额外交出指定真实下标读取点之前的NumericEffect。已证明该点与本次真条件仍是同一数值时，原必要谓词使用现有ElementIndex相对当前Data.elements表达，经原loop_write_projection按真实dataset层级交给Write::Position.when；已有实际Operand保留，不伪造SSA、物理槽或地址。原路径实际不经过该写时交出不可达条件；未知、实际改写或受嵌套重入影响的值不复用旧谓词。整圈数值查询继续消费原回边结果，读点查询不改变它；Scope写回调不承担执行顺序或回边分析。词法声明只借用原AST，随本次查询存活，不复制资源模型、执行图、拥有者或运行状态。带初始化的词法声明沿原Local借用显式类型（若有）、原初始化表达式及其后续源码切片，Scope不执行或复制源码。原numeric_loop_paths在真实while头取得声明到入口的NumericEffect；净零Shift保留原Proof，其余实际Shift由bounds::Proof的原u32数值范围所有者交出入口Proof，原ScalarRanges按类型及既有nonzero取可达值，integer_result_range／arithmetic_ranges交接加减的低32位及绕回两侧区间，不把循环回边的数学不绕回前提套到单次入口。该过程不创建Value、寄存器、SSA／物理槽或另一份数值状态；Assign／Reflect及未知前缀不借旧初值，条件求值的实际修改关系仍由原表达式效果处理。原iteration_bound／bounded_ranges继续消费必要条件及回边、continue或无回边的单次体，给出共同次数与相应体域，再按实际dataset层级投影原Write::Position.when；实际写点仍查询同一原数值遍历。条件块内／跨条件块的新鲜声明与嵌套重入沿原借用来源交接，不复用失效初值，不增加AST解释器、Scope状态表或运行状态。初始化后的赋值及其条件合流与祖先已拥有标量入口仍分别见Issues C-020.1.3.2.1.2.2.13.4.1.10.1.1.2.2.2、C-020.1.3.2.1.2.2.13.4.1.10.1.1.2.3；形式数值谓词不提供写引用的物理存储。原Scope::local_value_write按真实词法声明、普通非拥有值和原布局投影交接独立局部位置；直接赋值与write／inout调用共用该查询，记录字段、固定数组元素、地址及函数值按各自普通存储处理。普通B闭合调用中，直接Root::Parameter存储写在预分析时投影回对应的原Write::Position，由该原位置描述这份直接写；其他形式内容中的地址／借用、资源元素、外部及不能闭合的内存效果继续由原Memory交接。拥有结果及Element／NamedElement不被当成独立局部普通存储。实际调用仍使用本次求值取得的物理位置及同一合同，不生成未来SSA／物理槽、来源身份、状态表或第二执行器。不增加模型副本、位置身份、ABI、表或运行状态。普通前向游标的逐次读前数值由原当前绑定持有，循环头的未来写与当前语句的实际写分别按其生命周期消费；整份共同元素模型仍按真实写更新。跨调用数值输入／输出与未来Memory读取投影的剩余范围分别见Issues C-021.1.4.5.4.2.2／C-021.1.4.5.4.3.2.2，不以本字节查询或一次游标输入的结果泛称全部循环／清理能力。原词法Scope已声明的纯值局部计数器更新不会改变元素数值前提，直接赋值和嵌套循环按同一局部身份解释；真实元素别名与资源写仍按实际位置失效。一般循环剩余数值前提仍按Issues具体缺口处理。分支、循环与退出只清理仍拥有的资源叶；未初始化或已移动资源、缺少普通字段初始化、重叠借用及非法回边沿原诊断拒绝。函数实参和记录构造字段从左到右求值，先前产生的临时拥有值在后续 try 错误退出时随原作用域释放；成功才转交给实际调用或构造结果。临时 dataset 的 read／inout 借用保持其拥有者到本次真实函数／计算函数 调用结束，GPU物理寿命仍沿原最后消费者，不能用 CPU 作用域结束冒充设备完成。

固定拥有数组的动态部分移动、整份拥有聚合read／write／inout、条件初始化及CPU自包含拥有view聚合均按对应章节使用，不扩大普通C ABI。拥有元素子资源的完整长度关系／批量增长、一般循环载荷／位置不变式、cooperative整块提前返回及完整资源阶段仍是缺口。

## 固定数组拥有资源与动态借用
普通 CPU 应用支持 `array<dataset<T>,N>`、拥有记录／Result 的固定数组，以及与记录、Result、另一层固定数组互相组合。数组按元素对齐步长保留全部普通字段和填充；拥有性由实际元素类型递归决定，不能把句柄当普通字复制。例如，在已经声明 Row 的 CPU 应用中：

```c
let work = array<dataset<Row>,2>(dataset<Row>(65), dataset<Row>(66));
for (i in 0..work.length) {
    println(work[i].length); // 借用真实选中资源，不移出元素
}
let first = work[0];
work[0] = dataset<Row>(67); // 补回后可再次移动整份work
```

构造、按值形参、返回、同型构造与赋值移动整份拥有数组；常量或既有事实确定唯一元素的下标可部分移动、初始化与重赋值。只释放退出路径上仍拥有的资源叶，未初始化或已移动元素不释放；Result只释放活动载荷。普通字段继续按值复制。拥有数组、Result和临时嵌套投影使用同一 Frame，临时下标／成员的基对象与下标只求值一次。数组元素类型也为未显式写类型实参的Ok／Err及其条件选择提供上下文。

有界动态下标可取得实际 dataset 的 length、CPU元素位置和游标，或按原read／write／inout／atomic规则借给实际调用。借用保留所有可能选中的真实资源来源，不能按两个新寄存器名假定资源互不相交；不充分的区分仍按原重叠规则处理。已准备的实参、元素下标、数组子位置和赋值右侧求值期间，资源所有者不能被后续表达式移动、覆盖或传输；游标结束后借用退出。CPU／GPU位置来自实际候选资源，混合位置不能授权访问；不隐式上传、下载或等待。

数组按值交接和资源位置合同可经过独立B对象／归档，消费者不读取实现。动态选中位置的移动／覆盖、同位置补回及可证明循环初始化见“函数内动态拥有位置、补回与循环初始化”；整份拥有聚合write／inout及CPU自包含拥有view聚合见对应章节。一般引用返回、拥有元素子资源的完整长度关系／批量增长和任意初始化覆盖仍待实现。N为u32编译期长度，包含0；零元素不产生拥有或借用分量，仍保留原元素类型与对齐。零尺寸普通值保留B源类型、参数身份、求值顺序及副作用，不占机器值／引用参数槽、复制地址或业务结果地址；后续非零参数按连续实际槽传递，可失败调用仍交回原错误状态；普通C ABI与address_of的实际范围分别见互操作／地址章节，更广形状继续开放。

## CPU 应用普通函数的原生调用
CPU 应用主流程的普通调用生成独立原生函数、调用帧和返回；不再展开每次调用的函数体。函数无需新增标注，自递归和互递归使用相同名称解析、类型及访问规则。例如：

```c
u32 factorial(u32 n) on cpu {
    if (n == 0) { return 1; }
    return n * factorial(n - 1);
}
```

当前Windows x64实现覆盖u8／i8／u16／i16／u32／i32／u64／i64／f32／f64／bool、已有向量、记录、内联数组、结果与void的参数／结果；大于直接寄存器布局的普通值使用真实按值缓冲和隐藏结果地址。write／inout 直接借用原位置，字段、数组和合法重排分量沿实际映射编译，不复制借用内容。requires 仍在调用点静态证明，write 在所有实际返回前完成；调用后的输出事实和可写存储失效沿原 Proof／所有者交接。

拥有 dataset、记录和 result 的调用传递原 Frame 句柄／长度和活动资源叶，同一可变存储只有原所有者。资源的 CPU／GPU 位置按静态输入条件与返回关系交接，一份具体函数不再按驻留位置生成`#s`符号；返回 GPU 存储不会下载；原拥有者显式 download 后才可在 CPU 读取。应用入口拥有 Frame 创建／销毁，需要资源的函数链只借用同一 Frame；普通纯值调用不增加执行框架依赖。

compute 值函数从 CPU 应用调用时可生成 CPU 原生函数；parallel 元素函数、整域算法和 cooperative 仍使用原参与范围及专门化规则，不能由主流程递归能力推导设备递归已经实现。一般递归资源的存储位置事实、拥有参数资源阶段合同、其他目标普通调用及选择性内联继续研发；未知资源位置保持未知。独立模块与多对象链接按下节的实际有限范围提供，不能当作完整模块ABI或稳定热更新。

### 普通函数的位置合同与返回关系

`on cpu`规定普通函数控制流的执行位置，不规定它携带的全部资源必须存放CPU。资源借用／拥有实参、length及返回交接不读取载荷；真正的CPU索引／循环、`target(cpu,...)`、`target(gpu,...)`及显式传输才提出相应条件。upload要求源CPU，download要求源GPU；二者仍仅能由拥有者执行。

编译器以参数序号和嵌套记录／result的逻辑字段路径推导输入条件，并沿原原生模块Contract导出；调用点用真实资源满足它。未绑定的资源输入与数值／内容输入均沿本函数原参数映射及原Contract持有；定义只消费已经实际绑定的调用，仍依赖源输入的原Call、数值与路径由实际调用者代入验证。Frame自身不是源形参，不为声明输入枚举递减数值域。原使用点先交接生效路径，再解释该路径实际可能的资源位置分支；只问来源存在的消费者复用原存在性查询，不构造完整位置树，空解Absent与真实未知继续区分。循环入口／回边的实际位置相等、独立CPU范围的全部Call与deferred、GPU显式传输及实际错误继续沿原消费者检查。递归调用的输入条件沿原调用关系传播，包括参数交换。每个具体类型／const实例只生成一个函数符号，混合CPU／GPU记录和result的不同逻辑载荷保留各自关系，不依赖物理union槽生成另一套身份。

拥有返回可保持输入位置、确定的CPU／GPU位置或多条真实返回路径的可能来源；调用点代入实际输入。result中不存在的载荷继续由原载荷元数据表示，不变成位置未知的活动资源。未知位置不能进行要求确定位置的访问；未知递归资源结果也不会被当成CPU。循环显式upload后download可回到同一输入CPU位置，一般动态条件相关的资源阶段、借用返回、异步及版本能力仍未完成。

模块接口保存这些位置条件及返回关系。无库.h／.b的消费者可调用已编译具体函数；合同相同的私有实现更新可重链原应用对象，位置条件变化则原对象不能绑定。原Frame和资源唯一所有权、显式传输与真实完成保持，不能把本项当作完整资源阶段或性能结论。

### 原生位置参数的内容事实

普通CPU函数的inout入口按真实位置的逻辑分量记录已有内容，地址输入与内容身份是不同关系。函数未改写的字段、数组元素及向量分量在正常返回后继续保留原范围、索引域、非零及相等事实；实际写入的分量只交付函数所有正常返回共同保证的事实。参数交换、从一个借用复制到另一个借用、普通返回及write输出均按各自入口内容翻译，不按调用后已更新的位置反查旧值。整型二元运算的原Computed关系随实际结果Proof交接到返回及write／inout合同；无符号余数再除原同一非零除数时，若结果仍持有该Remainder关系且除数的实际Operand快照或同型常量身份相同，范围为零，原空路径仍为空。不同除数、除数改写后的新值及未保留该关系的未知输入不获得这一结论，浮点归一化不由此取得整数索引许可；dataset长度分配、传输或返回时的资源域重绑仅替换存储身份，保留同一实际长度的整数表达式。真实覆盖以新值为准，未知或回绕为零的结果不能作为非零除数；调用按真实输入绑定，精确点沿原常量语义，已知区间复用原arithmetic区间运算并与原输出范围相交，保持实际位宽、符号、回绕与移位语义。静态生成器下标在零至一时，i+1输出一至二，可进入同一共同元素非零合同；可能回绕到零的输入不能取得该许可，数组索引与非零检查继续消费这些事实。输入快照不由写后位置替代，实参仍只求值一次；未知操作数保持关系或未知，不猜测为常量，也不为取得事实重读提供者实现或重建未保留的私有中间计算。

例如Ticket含index与side，touch只修改side时，下述调用不会抹掉ticket.index的既有索引事实：

```c
void touch(inout Ticket ticket) on cpu { ticket.side += 1; }
// 当前ticket是已初始化CPU dataset中的实际位置：
if (ticket.index < seen.length) {
    touch(ticket);
    seen[ticket.index] += 1;
}
```

返回bool谓词也可以引用尚未由调用者读取的入口分量；只有该分量在返回后仍保存相同内容时，谓词才精化它。函数先读取旧index再改写index，即使返回旧值的判断，也不能为新index授予索引或非零事实。write参数没有可读取的入口内容，仍须在实际返回前完成初始化；真实重叠的独占参数仍拒绝。

这些关系随既有编译模块合同导出。调用者不需要库源码；私有实现改动后，只有实际导出静态合同仍相同，旧调用对象才能直接重新链接。此处不增加运行期载荷复制、内容装载或机器参数，不改变原借用地址、正常返回、资源所有权与完成规则。C可写调用继续按既有外部权限撤回旧内容，不从外部名称猜测保留关系。

当前范围限于既有位置模型能投影到原所有者的分量。递归环中的未知写后事实、一般动态数组位置关系、完整字段requires／ensures及一般资源契约继续待实现；不能据此宣称完整C-005或任意别名／递归推导已经完成。
### bool形参真值与返回谓词

标量bool参数的真／假路径参与既有静态事实交接。函数返回参数本身、取反、AND成立路径、OR不成立路径，或通过实际分支返回bool时，调用者可以继续使用这些路径共同要求的索引、范围和非零事实；bool的普通返回、inout及write输出沿相同规则处理。例如：

```c
bool both(bool index_ok, bool divisor_ok) on cpu {
    return index_ok && divisor_ok;
}
// item是已初始化CPU dataset中的当前实际位置：
if (both(item.index < seen.length, item.side != 0)) {
    seen[item.index] += 1 / item.side;
}
```

交接的是实参求值时所观察的值。数值或位置在后续实参、函数体或返回后被改写，旧bool结果不能给新内容授予索引或非零事实；独立保存的bool副本仍描述原值，不因另一bool变量重赋值而改成新条件。未初始化write输入与不充分／无关条件继续拒绝。

同一bool值不能同时为真和假，这样的路径不会提供正常结果；原函数调用与全部实参的可观察副作用仍按真实顺序执行，不因结果静态恒真／恒假而删除。上述必要事实随既有编译模块合同导出，调用者不需要私有实现，也不增加运行期条件缓存、额外装载、检查或机器参数。

OR与条件选择保留各自的合取／析取关系，在调用边界先代入实参当前持有的条件，再提取所有可能路径共有的索引、等式、数值上下界及非零事实。比如seen由dataset<u32>(65)创建时，either(item.index<seen.length,item.index<65)的两个成立分支均可证明同一下标安全；经分支返回、组合谓词、取反及bool的inout／write输出交接也遵循这一规则。只有一侧提供足够事实的either(index_ok,true)、过宽上界或已改写内容仍不获得许可。内部以共享的不可变条件图保存关系，事实提取与实参替换逐节点消费，不展开为析取路径的笛卡尔积。

已编译模块中包含条件的静态合同改用带格式区别的共享节点表，两个结果共同引用节点；这属于原模块接口的编译期表示，机器参数与布局保持。含旧条件编码的对象需要用相同编译器重新编译，新旧编译器在原接口解码处明确拒绝混用；不隐式转换旧合同。相同新格式、实际导出合同相同的私有实现修改仍可直接与旧调用对象重新链接。

### 结构化bool与整数顺序事实

bool条件随普通值的逻辑分量交接。记录及嵌套字段、固定数组的已知元素、向量比较所得bool分量和重排、普通Result的活动bool载荷可以保留原索引、范围及非零条件；构造、赋值、按值形参、普通返回以及write／inout输出共用同一规则。部分写入只替换对应分量，未改写字段继续描述原快照。例如：

```c
struct Flags { bool index_ok; bool divisor_ok; u32 tag; };
Flags guard(u32 index, u32 count, u32 divisor) on cpu {
    return Flags(index < count, divisor != 0, 7);
}
// item是已初始化CPU dataset的当前实际位置：
Flags flags = guard(item.index, seen.length, item.side);
flags.tag += 1;
if (flags.index_ok && flags.divisor_ok) {
    seen[item.index] += 1 / item.side;
}
```

整数严格／非严格顺序比较可以保留形参间的关系，待调用边界代入真实实参后再取得数值端点和无符号资源索引域；包括u32／i32及原向量比较对应的分量。普通整数参数不会仅因命名为count或length就取得资源身份；同一实际值关系、现有资源长度来源及当前路径共同提供事实。比较的反向／不充分上界、无关bool分量、真实改写和可写外部调用仍撤回或不能建立许可，原函数及实参的副作用照常执行。

选择与Result的活动分支使用已有环境中的真值和数值端点，先排除与当前路径矛盾的条件再提取共有事实。Result保持原bool判别字；内部32位载荷存储保留其逻辑bool条件，不把用户的普通bool→整数→bool转换新增为关系恢复规则。未写位置的入口bool条件可随原内容身份交还，不为它增加载荷读取或机器参数。

这些条件仍只由原Proof按逻辑分量持有，所有分量的false／true根共享一份不可变节点图。编译模块条件载体用明确格式区分原标量合同与分量合同，拒绝误读，不转换旧条件编码；机器参数与数据布局保持。资源位置合同随B对象交接；对象合同与资源ABI分别匹配，不将局部条件序列化的对照结果外推为完整对象兼容。

一般动态数组跨次读取的相关位置、任意布尔／算术关系及完整循环不变式仍待实现。正常路径及返回条件按下节交接；循环数值分析只提供其明确支持的入口与继续边数值界。

### 数值分支的正常返回条件

现有控制流持有正常路径的共享条件图。字面布尔值、原不可变快照及完整循环写摘要已证明稳定的本地bool的原Proof已知值，以及&&／||组合的确定结果由原Scope写扫描交接，循环写集合仅保留可能执行的if及?:分支；NumericEffect消费同一if选择，不产生未执行正文的写入、break或continue。条件自身可能执行的调用与访问仍按原顺序保留，结果确定不等于条件没有副作用；完整短路AST及两侧类型／访问检查保持。其他条件沿原求值及路径规则处理。分支分别继承入口条件，仅合流真实继续边；ReturnContext在实际返回点把路径条件交给bool逻辑分量与write／inout输出，LoopState沿自然结束、break与continue交接路径。这样if(index<count)后返回true／false或常量bool字段，仍能在调用点代入实际资源长度；OR不在代入前压成共同端点。当前值Proof与执行路径各自负责值快照和实际执行条件，改写后的内容不会继承旧观察。复用原Clause及模块合同编码，不增加运行时状态、检查、机器参数或后端路径；一般关系求解、循环回边不变式和分开读取的动态位置关系继续开放。

### 动态下标读取的事实交接

固定array的动态读取保留各可能元素的bool条件，以及数值范围、非零、索引域与共同来源。局部值、临时构造、嵌套数组／记录、按值数组形参与inout位置借用遵循同一关系；实际下标范围会排除不可能元素。比如从array<u32,2>(1,2)动态取出的除数仍是非零值，不必改成常量下标；从两个都证明同一资源下标安全的bool元素中选取，也保留该条件。

对同一未改写数组和同一已绑定动态下标的重复选择，条件和再次读取共享实际所选值关系。例如先绑定`let k=...`，再以`value.indices[k] < data.length`证明后，可用`data[value.indices[k]]`；普通按值记录、实际位置、不可变view行、临时向量值及独立模块沿同一规则。一次动态选择以整个实际元素为单位交接全部字段；候选证明消费读取前的路径、原内容事实、本候选条件及该候选的实际结果绑定，直接保留同一记录字段间的共同身份与条件，再合流共同事实。完整候选关系一次交回原路径，供后续条件和读取使用，不作为每个候选反复求解的输入；先前读取关系仍由同一原路径消费。若各候选记录的两个字段实际相等，对选中字段成立的范围同样适用于另一字段；若记录的布尔字段保存其索引字段小于同一数据长度的结果，选中记录后可在该布尔条件内消费相应索引。改写字段或实际来源后不能沿用原关系。原位置尚未知的候选字段身份只保存在原内容Proof，没有运行时缓存或隐藏读取。改写下标、改写实际候选内容、退出条件或经过可能写入的循环回边，均不能套用旧事实；未改写兄弟字段保持原关系。整数二元运算按实际标量类型、运算及已求值输入快照交接结果关系；相同输入上的同一确定运算可在原路径中证明结果相等，故`value.indices[ordinal(item)&1]`的重复选择不要求先手工绑定下标。嵌套运算沿实际结果关系组合，不按源码文本合并求值，不重排或删除调用及原子读取；输入或候选内容改变后不能沿用旧选择。此规则不是任意代数归一化，也不改变浮点运算许可。

同次读取的记录字段来自同一元素，其数值和条件不能在合流前拆散。array<Candidate,2>的两个元素分别持有不同key及各自key<seen.length的valid字段时，取出一个Candidate后，if(chosen.valid)可以证明seen[chosen.key]安全。读取后加入的槽位约束也参与原条件图必要事实的继续传播，不因它出现在数组谓词之后而丢弃。

索引有效性、初始化、可写权限和内容失效仍由原规则负责；未知或不充分条件不会授予访问权。分开读取两个相关数组仍有合法B137缺口，记录为open_split_arrays；cpu_open_return_record的新输出身份缺口已由下节修复；一般回边不变式和完整关系求解继续待实现。本项沿用原模块合同及条件编码，不增加运行期检查、缓存、装载或机器参数。

### 普通返回值与写后内容的关系

普通函数返回的新数值分量与bool字段按实际值身份共同交接。例如返回Entry(k,k<count)后，调用方使用该valid字段，可以约束同次返回的key；多个正常返回点、嵌套记录、固定数组、Result活动载荷及普通write／inout输出遵循同一规则。返回值与写后内容之间的关系也可交接，例如返回key并写出key<count，或返回条件并写出key。整数非零关系同样可以用于u32／i32除法。

入口值、旧快照及改写后的内容分别保留实际身份。重复返回同一个值不会把它变成两个无关值，后续只改写一个字段也不会改写另一个字段的事实；改写key、不充分上界、错误资源或存在不安全返回分支时，旧条件不能授予访问权。静态关系不删除函数或实参的可观察效果，也不改变求值顺序。

各正常出口先绑定到共同的真实返回分量和写后内容，再合流路径与事实；原模块合同将这些编号代入调用点实际结果。机器参数、真实装载、数据布局及资源所有权职责保持。旧编译器产生的模块缺少此处新输出关系时，调用方仍可能报B137，须重新编译该生产模块以导出实际关系；不从源码外猜测缺失事实。跨次相关数组读取、一般循环不变式及完整关系求解继续待实现。

## 普通递归函数的资源返回
自递归和互递归可以返回拥有dataset、拥有记录及result，继续按原所有权移动，不复制载荷或隐式传输。例如：

```c
dataset<T> relay<T>(u32 depth, dataset<T> rows) on cpu {
    if (depth == 0) { return rows; }
    return relay<T>(depth - 1, rows);
}
```

上述CPU控制函数保持输入存储位置；调用者可以交付当前CPU或GPU存储，再在同一真实位置使用结果。函数若读取载荷、发起计算或显式传输，则调用闭包推导其实际输入要求和输出位置，参数交换、记录字段及result的两个逻辑载荷分别交接。循环回边仍需归还入口的实际位置，拥有／借用和初始化义务保持。

正在编译的递归声明不会把返回永久清为未知；编译期沿实际调用关系闭合来源，再完成访问与传输检查，不展开递归深度。真实未知合同仍拒绝不成立的CPU／GPU使用；没有实际资源来源的结果分支也不伪造资源。该规则不证明递归终止，不改变普通值条件／范围的能力，也不允许把借用返回为拥有值。

无实现源码的模块／静态库交付闭合的位置合同；库内部修改且合同相同时可重链原调用对象。公开Contract在同一出口统一真实值／存储命名、类型化整数条件及完整同目标要求，有限同义读取条件在局部布尔、合取、嵌套if和多个返回点之间改写时，不再按内部Clause构造形状区分身份。规约保留原真假两根、数值范围、借用与实际存储版本；返回、驻留及访问字段共用同一遍历，不为每个字段建立另一身份。只有原已取得的数值关系参与规约，不推断任意算术或任意程序等价。编码沿现行对象格式的原字段与标签，原机器ABI、Frame及执行映像格式保持，不带入运行期位置表。真正不同的读取条件仍须使用对应合同，不能仅凭相同.h忽略差异；链接继续严格比较完整合同。固定拥有数组和CPU自包含拥有view聚合按本文对应章节交接。一般资源阶段、一般位置引用返回、持久dataset图的B构造、符号模板、版本和热更新继续待实现；具体原生范围见Issues。

## 独立原生模块
将普通CPU适用函数库编译为模块对象，再让应用只消费已编译接口：

```powershell
.\target\release\bc.exe -c sample/beginner/math.b -I sample --module -o target/modules/math.obj
.\target\release\bc.exe -c sample/beginner/modules.b -I sample --import target/modules/math.obj -o target/modules/app.obj
.\target\release\blink.exe target/modules/math.obj target/modules/app.obj -o target/modules/app.exe
.\target\modules\app.exe
```

输出13。模块产物保存实际.h的公开声明／常量及已编译具体函数的布局、访问、结果／写后事实和原生符号；应用不需要该库的.h或.b文件。依赖模块可通过重复的`--import`提供对象或下节的模块静态库，并与其余源码依赖同时使用；已提供模块不需要分发.h／.b，其他源码依赖仍须有实际输入。最终链接列出本对象及实际对象／库，成员按真实符号需求引入。归档目录只选候选成员，请求符号的实际定义及合同来自该成员；目录命中而成员未定义请求符号时，链接和栈报告沿原错误结果说明请求符号与实际模块，不写出产物，也不改选其他库。缺失、重复或不兼容定义直接报错，未被消费的成员不作语义预扫描。未提供任何对象时就是原完整源码闭包，全部依赖均提供对象时无需库源码；两者共用同一按模块选择，不在失败后切换模式。普通应用、独立模块及`--program`算法库共用上述逐模块来源选择。算法库可消费已编译接口中的类型／常量，与其他真实源码依赖组成原CPU／GPU算法；未消费的同名.h／.b变化不失效，所用常量／类型或源码实现变化正常重编。CPU算法范围及自定义归约／扫描可调用已编译、无需Frame的普通CPU函数，沿真实合同交接值、记录／数组／Result返回和read／write／inout／atomic位置；递归、已编译具体泛型及传递依赖继续使用原模块符号。编译产生同一算法对象的导入，链接消费所选原COFF或静态库成员及真实外部DLL导入；缺失符号或不匹配合同直接报错，不读取同名.b。CPU范围可按本实例的真实权限，将dataset或字段投影借给无需Frame的普通read／write／inout／atomic参数，沿原物理来源、步长、requires、实际写入与借用寿命交接；write按本次实际元素与字段交出初始化，Count关系按真实可能写区间撤回。局部分配、拥有结果及内部计算所需的真实Frame资源交接仍待实现；GPU位置调用普通CPU符号报B161，独立入口映像不接受COFF导入。

算法DLL与B应用模块的CPU范围及集合入口均编入所属对象的COFF代码节、符号和Windows展开信息；原链接器完成普通模块、外部C接口及输出入口的重定位，原CpuCode只借用已链接地址执行同一阶段／线程调度。B应用模块工厂以C布局交接静态资源字节与已链接代码地址表；应用不再另拆blink_app支持DLL，原标准CRT入口调用同一B正文main并初始化必要平台设施；字节只保存类型、执行布局、CPU入口索引和SPIR-V，不再保存本路径CPU机器码或执行运行时复制／重定位。CPU索引编码使用标签2，独立SDK原NativeCode映像使用标签0，GPU为标签1；不能把索引映像当可独立装入的机器码。Frame借用原目录中已绑定的CPU代码，GPU从同一只读映像在本Frame设备装入，所属PE／DLL及其代码依赖保持到最后CPU工作和程序消费者结束。独立SDK动态映像仍由原VM所有者装入。

模块导出公开的有限具体实例，开放泛型可以在模块编译时重复指定`--instance 'function<args>'`；尚未导出的开放实例不能靠装入私有源码临时生成。普通CPU函数可包含同步CPU／GPU程序、返回拥有资源，跨对象仍借用同一个Frame；显式upload／download、设备真实完成和最后消费者释放的语义保持。

外部拥有实参使用限定类型及静态F，例如--instance 'push<owned<items::Item,items::dispose>>'，并以--import提供items的真实对象或归档；只从该模块公开接口取得声明，不读取其.h／.b。实参在请求上下文解析，实例正文仍按原库的词法作用域检查。缺失／私有／大小写不匹配及错误释放签名仍拒绝；库无需导入用户模块，普通源码的名称可见性不扩大。

CPU普通位置的跨对象借用现按逻辑分量／声明布局直接交接原地址，不再依赖调用方物理映射实例。借用返回、一般拥有参数的CPU／GPU阶段合同、符号泛型模板、完整常量／依赖版本、设备普通跨模块调用及热更新尚未实现。含资源对象按下节的模块资源C ABI与已编译映像交接；Frame私有布局及无关SDK源码不参与兼容身份。链接不会编译实现或掩盖未知资源位置。

## CPU调用链的静态栈报告

`bc -c`可同时写出本对象的独立报告。Windows入口在原报告写出期间只读持有已发布的COFF，实际同文件的报告写入由系统在截断前拒绝，包括同路径、大小写别名及硬链接；原对象继续可消费，错误返回时不报告本次整体成功。独立新报告与既有报告更新保持；报告目的地直接交给原文件入口，目录创建只消费实际存在的父路径关系。`blink --stack-report`从已编译应用、模块和归档解析实际选中的调用图，只生成报告。Windows独立报告入口在读取前取得各输入文件的只读共享句柄，并持有到报告写出结束；报告目的地若指向任何同一输入，由OS在截断前拒绝，原对象／归档保持可消费。报告直接使用原输出路径，失败沿原结果返回；不建立路径身份表或替代写出。对象恢复沿原COFF节与重定位解释栈位置和直接调用记录，重复或无效位置及缺少对应重定位的引用会交回读取错误。后者不链接PE、不读B实现、不调用Cargo／rustc；需要PE时仍单独使用原链接命令。例如承接上节的对象：

```powershell
.\target\release\bc.exe -c sample/beginner/modules.b -I sample --import target/modules/math.obj -o target/modules/app.obj --stack-report target/modules/unit-stack.txt
.\target\release\blink.exe --stack-report target/modules/app.obj target/modules/math.obj -o target/modules/stack.txt
```

编译期报告将对象之外的普通模块调用列为unresolved；独立报告沿原链接选择、符号与节映射闭合它们。未选中的函数及归档成员不参与报告。机器帧由原发射器交付实际分配区和保存寄存器，CALL记录引用同一机器重定位；函数地址值不冒充调用，间接调用明确未知。打印辅助、导入跳板的已知帧同样参与，外部代码、执行运行库及其线程的栈范围继续由真实所有者提供。

报告逐函数列出Local、Known path和Upper。Local为真实局部／保存／出参区，不包含自己的八字节返回地址；调用者准备的入参／home空间归调用者，不能再计入被调用者。行从调用指令前开始，Known path计入返回地址并在未报告目标处停止，不是完整执行上界。只有实际选中图的全部可达调用均有报告、且没有调用环时才给出Upper；独立顺序调用取峰值，不把它们相加。递归及互递归标为未建立有限上界，不据图中的环断言某个实际输入无限递归；普通间接调用、外部DLL及原运行库都不猜成零栈。

可选`--stack-budget <字节数>`由调用者声明在准备好其参数／home区后、独立调用该行函数前真实剩余的容量，只用于报告：本帧连返回地址已超过时标明if entered；闭合Upper不超过时说明该模型上界满足；仅保守Upper超过而本帧未超过时，实际路径仍未证明。没有预算就不作满足／超过判定。链接报告另列按原PE策略形成的reserve，它不是任意线程／调用位置的剩余容量，不自动充当预算。工具不拦截执行、不改递归深度或PE容量，不替代编译器内部耗尽定位、B运行时耗尽定位或源码调试。

原生Rust SDK的`Object::stack_report(Option<u64>)`查看该对象，`linker::stack_report_inputs(inputs,Option<u64>)`查看同一实际选择／组合结果；`None`不声明预算。报告元数据随当前COFF交付，缓存对象保留同一信息；旧格式对象须用当前工具重新生成，不读取旧源码补填或猜测旧对象的调用图。资源C ABI及映像版本仍见下一节。

CPU入口的局部帧数值统一由原NativeCode.stack_bytes交接实际分配、保存寄存器与出参空间，不含本入口返回地址；普通函数的对象Frame及PE输入消费同一字段，不另存一份local_bytes。范围及自定义集合的五次64位保存计入40字节，普通函数仅在实际使用scratch寄存器时计入其8字节保存；手写集合入口各按真实指令统计。应用、独立模块与算法DLL的CPU执行映像将每个Element主入口的同一字段交给对象报告，行名为CPU、程序身份与stage序号；这些入口没有静态COFF位置，不伪造地址。同一NativeCode保留发射器在实际invoke处记录的调用：Some位置对应该代码的真实调用重定位，None对应真实间接CALL；只取得函数地址的重定位不增加调用。无调用入口以局部帧加8字节返回地址闭合本入口Upper；具名外部／运行库／模块目标本体与间接目标仍未知，已知前缀同时计入实际CALL的返回地址，不把未知本体当零。本帧已超过所给预算时仍如实标注。模块资源未被选中时相应行一并裁掉，GPU专属映像不产生CPU行。四种单入口延续ReductionTail、ScanAdd、CompactScatter、SegmentedFinish也从同一映像交接局部帧，行名分别注明reduction tail、scan add、compact scatter、segmented finish；它们由运行库在主范围完成后独立调用，不与主入口的局部帧相加。ContinuationCode::Scan的双入口也分别按scan prefix／scan apply交付同一映像中的局部帧；自定义扫描和自定义分段归约共用这两个字段，宿主先调用prefix再独立执行apply，不把两帧相加为线程上界。Sort映像的全部initialize入口及histogram／scatter／gather也各自交付原局部帧，行名分别注明sort initialize与从零开始的映像次序、sort histogram、sort scatter、sort gather；初始化个数来自实际展开键列表，不固定为源键数，不按运行时基数轮次重复记录。运行库独立调用这些入口，不把帧相加。这些入口的实际调用／无调用事实已交接；运行库本体、间接目标与实际线程的完整调用栈仍归原所有者，未知不作零栈。

## 不透明C内存访问合同

外部C声明与cfn签名可用ensures memory(none)、memory(read)、memory(write)或memory(inout)声明本次调用的不透明内存效果：none不访问，read只读，write只写，inout可读写。未声明时为inout。多个ensures是同时成立的承诺，允许的方向取交集。这里的memory是原访问合同中未由带布局位置参数表达的不透明部分；显式read／write／inout位置仍按其实际C参数、初始化与输出合同交接，不因不透明部分只读而丢掉明确位置的写入。

这是真实外部ABI的信任边界，声明者负责外部实现符合承诺；不对C正文作验证。普通B正文沿原实际访问推导，不以该外部承诺隐藏写入。合同随cfn身份和独立对象交接；函数地址的非零与同签名不证明外部实现、代码或异步资源寿命。调用、真实读取、原参数顺序、错误、资源拥有及最后完成保持；none不表示纯函数、确定结果或允许删除调用。不增加运行时内存检查、来源登记、API白名单或另一份效果图。

原MemoryTarget／MemoryEffect同时保留不透明读写方向与真实位置效果；未知读取加一个已知位置写入不扩成所有存储未知写入。调用后的数值事实、实际内存装载及优化分别消费原显式Reference位置和额外内存效果；资源／Memory只读不代表普通标量Reference无写。已知局部槽及Indexed字节写按原位置处理，不因C typed位置的合同尚未闭合就再制造不透明写；memory(none／read)与其他元素的显式写不会单独抹掉未写下标的原常量快照。原位置记录仍进入同一别名／资源求解，不能删除真实写或把未声明C效果视为无写。实际下标或载荷经引用写入后不沿用旧装载身份，仍有效的局部常量快照只沿原IR已求值写入查询。不透明写同时撤回当前拥有／借用dataset元素模型的旧公共数值事实，新装载不继承此前初始化的非零范围；先前捕获的普通值仍是快照，原初始化、资源身份及最后释放保持。未声明调用继续未知，不按当前位置值、参数类型或是否取址猜测无写入。

## 已编译模块的资源ABI

公开条件合同以原Proof的共享DAG节点和false／true两根交接，编码只保存已有关系并作节点结构去重，不先展开为全部析取路径。同一结构子索引与自身合取／析取直接交接该子索引，不增加无语义差别的父节点；事实和两根分别保留，读取仍沿原节点／引用格式，不增加格式拒绝或借用许可。公开位置访问在输入投影后保留同一两分支条件；数值事实、初始化、真实借用及实际写入条件继续按原规则消费。共享条件、已绑定的dataset元素类型引用及资源位置／来源合同随现行对象格式交接；旧格式对象及归档须由当前工具重新编译，原版本入口直接拒绝不同格式，链接仍严格核对完整合同。对象格式与执行映像格式各由原编码所有者交接，不以软件版本替代兼容身份。

当前B对象元数据为 **BLCO91**，模块资源C ABI与执行映像为 **BLRI52**。对象格式由`object/coff.rs`交给B元数据读写器，模块资源ABI沿`resource::ABI`交接，各自在原读取入口拒绝不匹配的产物。对象格式保存上述有限地址读取与入口／回边关系，同时标识普通B零尺寸值退出机器参数槽与业务结果地址的ABI，并区分普通CPU纹理采样代码对新增R11格式标签的解释；不兼容的旧对象／归档须重编，源参数身份、真实副作用及失败状态保持。执行映像格式的资源C签名与映像编码不因这项普通B机器ABI修订改变。这里只标识实际对象合同、C签名、标量和映像字段／标签及语义；无关源码、SDK整体或Rust工具链不成为兼容身份。 本节集中说明现行版本，历史交付与原生证据见双台账。 DeviceRequirements编码保存f32／f64实际保留需求，ExecutionStage编码交接域、操作与执行布局；旧执行映像编码映像及包含该资源目录的对象须重编。dataset输出位置及其逻辑下标、借用Element的实际选择与Parameter共同输入族投影、调用处消费的本次实际初始化事实和CPU Invocation write实际地址参数由同一BLCO合同编码；普通只读纹理参数还按实际次序占据一个U64描述借用输入，未使用的纹理形参也保留该ABI位置，旧对象须重编。它们不改变执行映像格式的Frame／资源C ABI，不增加运行时初始化表。CPU ProgramImage在执行布局及阶段代码之后交接本映像实际消费的命名普通函数代码；同一具体实例只交出一次，不保存AST、IR或另一资源所有者。静态PE资源仍交接已链接入口索引，普通callee由所属对象持有；独立SDK映像交接原机器码、函数名及重定位。NativeCode同时交付主函数末端和Windows x64展开字节；末端不包括尾部无帧探栈叶辅助及字面量，装入器不从指令另猜栈帧。 函数代码及内部字面量以独立COMDAT节交付，展开pdata／xdata通过标准COFF节定义辅助记录关联到代码；符号索引按包含aux的真实表位置交接。函数签名与dataset元素以已绑定TypeName名义引用编码，实际记录布局、函数签名及元素存储分别交接；资源映像使用同一Ty编码。导入模块的实际类型绑定随原对象元数据交给消费者，源码缺席的obj／lib沿同一类型、资源来源及位置合同解析，链接不重读提供者实现。CPU纹理描述的format=4／5／6分别为原生RG16／RGB10A2 UNORM／R11G11B10 FLOAT，旧采样映像不能将新增R11浮点标签误按RGBA8／UNORM解码；执行映像格式沿这一真实格式解释边界区分，Frame布局和C签名未新增。普通CPU纹理借用函数可能没有Frame或资源归档，却已把格式分派编入机器码；故对象格式同步区分该对象语义，旧采样obj／lib须重编，不能只靠BLRI挡住旧代码；纹理绑定同时编码实际维度，CPU 每级描述交接宽高深和 64 位像素指针，stride24、像素 offset16；资源 ABI 按这些真实字段及标签语义拒绝混用，旧采样映像须重编。只因这些真实字段／语义变化更新对应格式身份，不因无关SDK源码或版本号变化递增。

普通CPU B函数的合同分别记录实际Frame需求与fallible；二者独立，由原Module的同一真实调用闭包收敛。资源分配、变长、执行、传输和可失败地址操作交回原拥有错误；只有分配／执行包装承担真实模块基址或参数数组布局，变长／传输／地址直接消费共同C入口；单纯释放、位置查询以及不含可失败闭包的函数不因此增加错误返回。无失败函数保持既有Windows x64值／位置及隐式Frame参数安排。可失败函数的RAX交回不透明拥有错误，NULL为成功；有源码业务结果T时，第一个实际机器参数为T* output，即使T为标量，Frame如需要则排在该输出指针之后，再按原次序交接其余参数；void不增加输出指针。callee只在成功时写实际业务分量，caller只在成功时读取，不读取未初始化结果或填充。源码的T／void与普通业务Result保持原义，内部错误不是第二份业务结果或Frame身份。

实际资源调用在原Call定义同次U64错误结果；同一Module只消费这个结果和普通B函数的闭合合同。FunctionBody.fallible与Call.error描述实际ABI，外部C返回方式不由库名或符号字符串推断；Raise交回原函数调用者，Exit仍只离开最近Scope。错误分支在原闭包闭合后一次降低到原If，不为已确定无失败的调用生成分支，不另建调用图、当前实现查询或错误队列。检查与Raise继承触发Call的source_site／generated_origin；ir_json交接fallible及error。外部C／cfn声明继续使用其明确签名，不自动改成此B内部调用。独立对象／归档以同一fallible合同选择真实参数和结果，无提供者实现源码时也不重编它。

普通资源失败先沿原拥有事实结束本函数尚存的局部及按值参数；独占write／inout拥有聚合由callee结束实际剩余树，caller只撤销本次真实借用位置的旧拥有关系，read不取得销毁权，正常返回保持原借用交回。记录及完整构造的容器由已接管的原释放组继续执行剩余F；F再失败时消费新旧两个拥有错误、保留全部原文本并继续，最后只交回一个拥有错误。

原Builder的env／path／initialized、实际位置及当前定义保持到同一Call／编译期Cleanup的最后消费者，原function_program以同一只读对象延长寿命，不复制调用图。原Module闭合真实可失败关系后生成普通If／Scope／Raise，最后统一优化和压缩编号；Cleanup及未使用的NoFail事实随后删除，不进入对象合同或执行映像。无失败调用不生成提前释放或额外F前提检查；正常释放组的可空错误与真实失败出口的拥有错误分别表达，不用正常清理取得借用参数的销毁权。

尚未交付的拥有结果在实际交接前仍归原作用域：本函数的返回准备值保持到同一次正常清理完成，普通调用已经产生的结果保持到原临时实参清理完成。二者沿原hold_binding进入同一env；正常清理只保留它，成功后才移出并交给原ReturnExit或调用表达式，清理F失败则由同一原失败Scope结束。return表达式不再单独重复结束临时拥有者，原finish_return统一消费该次完整退出。没有额外返回镜像、运行拥有表或完成标志；NoFail清理不增加失败出口或返回F的额外前提，真实失败清理仍满足实际F合同。

dataset(length,factory)逐元素构造期间，同一原作用域持有已分配存储，原写入完成后才推进的游标直接表示可结束的初始化前缀。factory真实失败时，原失败Scope结束尚未写入的实际返回拥有元素，并逆序结束已完成前缀的静态F及子资源，再由Frame的原入口释放该分配；未构造后缀不读取、不执行F。完整成功才交付原完整Data。共同元素模型消费factory原合同，实际待写值保留本次来源到写入完成；逻辑前缀不改变物理分配，不复制游标、拥有表或释放链。F再次失败沿原清理组保留实际错误并继续结束其他真实拥有者。resize取得的原输入在length与fill求值前沿原hold_binding进入同一env，原存储调用成功产生新Data后才移出并交付；后续实参或原调用失败时，原Scope结束该未采用输入。原存储采用检查以已有OwnedData Transfer表达本次真实输入，继续保留fill及其他未来消费者的实际借用，不复制拥有状态或释放链。普通实参求值期间沿既有hold_argument保持已求值拥有者；表达式完成后，native_call在引用／write旧值F等ABI准备期间仍以原hold_binding持有全部实际按值OwnedValue与ApplicationData。准备真实失败时原Scope结束尚未被callee采用的输入；准备完成后移出这些同一env关系，再生成callee Call的原失败Scope，callee进入后的错误不由caller重复结束按值参数。原Scoped read／inout临时拥有者及callee已产生的结果继续各按真实最后消费者交接，不新增采用标志、拥有表或释放链。闭合合同后插入的dataset写地址直接交回同一原存储位置，并由原Frame撤回真实相交的Count及共同元素关系；该地址准备本身不产生失败出口。resize的实际fill在共同元素模型捕获前沿原hold_binding保留本次F与子资源到存储实际采用；原调用扩容失败时，同一失败Scope结束尚未采用的fill与原输入，成功后移出两份原持有关系，只交给新Data一次。共同模型不代替待交付值，Frame的原失败结果先保留同一输入存储、长度与原错误，再由B失败Scope结束实际拥有者；F再失败仍沿原清理组继续结束其他真实资源；CPU范围及SDK的实例私有Frame与实际资源错误交接已完成原有限范围，证据见FixedIssues C-020.5.2.4.2.2.2。错误最终到达唯一应用根，再报告和结束Frame；源码业务Result保持原义。


view值为24字节、对齐8，五个U32逻辑分量依次为地址低32位（偏移0）、地址高32位（4）、长度（8）、实际元素步长低32位（12）及高32位（16）；20至23为尾部填充。CPU原生寻址与切片组合完整64位步长；GPU view由实际GpuDataset步长编码，地址仍指向原16字节dataset绑定头后的页数组。源B、独立对象／归档及宿主view图使用同一表示，明确on cpu的同源普通函数在CPU范围沿原native_declaration／native_call编译和调用，Compute共享函数保持原内联选择；CPU范围中的已有view可按原长度关系复制或切片，非零起点使用真实64位步长；不要求额外application上下文，存储与最后消费者仍由原拥有者持有。原来源和最后消费者继续持有真实存储。地址与长度的三分量身份供原范围关系使用，实际寻址同时消费步长，不把三分量身份当成整个机器值。真实布局与位置合同使用现行格式交接，旧布局产物须重编。

Rust SDK的Frame操作通过原Result交回错误。直接CpuTemplate::from_image／from_linked与Device::load_template的装入失败仍沿Err((error,image))交回原错误和同一未改映像。Frame::new消费原类型／program定义，在原模块装入阶段一次绑定CPU代码；构造返回Result，失败时未形成Frame，局部拥有资源正常释放。GPU定义只读保持，每个Frame按自身Device装入实际pipeline，失败不改变原定义；调用方显式选择后续动作，没有自动重放。run先借用同一原程序布局完成同根参数重叠检查；CPU代码已经绑定，GPU仅在本Frame首次需要时装入实际设备程序，再执行原阶段。参数重叠拒绝保持原定义、GPU实例、输入存储及域关系，后续合法调用仍借用同一程序。CPU／GPU驻留拒绝发生在实际执行／提交前；拒绝的当前输入和此前已取出的同类参数由原参数Vec交回Frame原槽，原定义与本Frame实际GPU实例均保持，未取出的后缀与原域关系保持。逻辑只读别名继续沿原selected只移动一份物理存储；调用者显式完成download或upload后可提交下一次合法调用，不自动传输、重放或复制原资源。CPU同步执行借用原参数拥有组，原错误与成功都交回同一存储，CPU代码始终归原目录；Frame::run在形成物理别名及投影前，直接从原槽元数据沿共同arguments接收本次逻辑参数；先拒绝错误数量，再接收类型、字段布局、形状和值参数。同次产生的参数字只交给CPU／GPU执行主体一次，内部投影入口不重新编码；独立CPU／GPU公开调用仍由原输入拥有者完成一次相同接收。输入域与冻结权限继续在执行访问前检查，参数拒绝保持原数据及有效域。参数接收后才沿同一Count关系撤回本次可能写计数字段的许可，指向未绑定数据的关系同样随其实际计数写入失效；只有全部阶段／延续真实完成才发布原prefix输出保证。线程池工作创建失败直接沿Result交回原OS错误；已有CPU写入仍留在原存储，不回滚或自动重放，也不将未完成调用当作新有效域。GPU调用准备借用原InputOwner，准备错误将原GPU存储及同一程序交回Frame原槽，已有域关系保持；错误不产生另一份资源或自动重放。命令录制使用原局部last记录本次阶段关系，物理producer与最后使用点只在成功提交后发布；成功才将原绑定移入Submission。Submission::wait成功交回Completed；完成等待或时间戳读取失败交回(error, submission)，同一Submission继续持有原绑定、加速结构、查询池与真实完成点。错误不承诺设备数据可用，调用者决定继续持有或明确结束原提交；原Execution与最后使用点继续承担实际消费者完成后的释放，不自动重放。已接受GPU提交后，Frame沿原Count关系撤回本次可能写计数字段的旧许可；等待失败从同一Submission取回实际存储，原程序定义及本Frame GPU实例保持，不发布本次条件输出保证。未写Count字段及无关原域关系保持，已有GPU写入不回滚；原GPU完成与最后使用点继续持有实际资源。upload借用原CPU输入，CPU暂存读取及GPU提交成功交回GPU存储后才替换原槽；驻留拒绝或上传错误不提前移除原输入，在途暂存区和GPU完成仍由原GpuDataset.last_use持有。address直接借用原位置，驻留或写权限拒绝不移走或释放原存储；成功取得地址后的范围、权限和期限仍由实际调用者遵守。resize在扩容申请失败时保留原存储；download借用Frame中的原GPU输入，仅在完整CPU结果返回后替换原槽，驻留拒绝或下载错误不提前释放输入。保留资源所有权不表示设备故障后的数据已经可用，实际设备失败与部分效果仍由原结果交接。已有Frame的六个共同C入口沿同一Result交回状态、成功输出和原错误；宿主持有原Frame并决定后续调用及显式销毁，错误不结束宿主。普通B显式捕获现行资源操作失败的结果能力仍沿Issues C-020.5.2.4建设；普通业务result的Ok／Err不捕获该执行错误，Ok(resize(...))也不把运行失败改成业务Err。当前resize的执行失败在Frame保留原输入之后，由原B失败作用域结束输入和未采用的拥有填充值，再传播原错误。显式捕获尚须定义错误及实际资源的拥有结果，不能在已清理后恢复；原操作的部分效果及设备数据可用性按上述各自合同解释。

编译期类型／program资源目录由原编译模块唯一持有，统一收敛后交给原应用／模块产物；函数的Frame参数寄存器只属于该函数，不携带或搬运目录。目录存在不能替代CPU范围的真实执行输入，实例局部分配／释放仍沿C-027.35.4.2.2.3.2.2。独立模块工厂返回C布局的静态资源描述ModuleImage{data:*const u8,bytes:usize,code:*const *const u8,codes:usize}，当前Windows x64为32字节、对齐8；代码表由原静态链接写入所属范围入口地址。非根包装只调用共同分配／执行入口，Frame是不透明指针，仅由根运行库解释和释放。工厂字节保存编译后的类型、执行布局、CPU代码表索引及SPIR-V，create由同一编译产物绑定到原Frame，CPU不重新复制／重定位，不重读B源码、运行值IR或重新编译。调用者不释放静态工厂字节或地址表，不跨模块传递Rust Vec／Box的私有表示。根创建入口为Frame* blink_create(on_error, context)，on_error的C签名为void(void* context, const uint8_t* data, size_t bytes)。接收函数和上下文由调用者保持到创建调用返回；错误UTF8字节只借用到本次接收函数返回，保留文本由调用者复制，创建调用不返回错误值；原创建错误字节借用结束后由创建者释放。首个模块解码失败后不再调用后续工厂，不建立Frame，原局部资源正常释放并返回空指针；运行库不结束宿主进程。B应用入口直接交付生成根中的blink_application_error符号，输出错误后将空指针结果映射为退出码1，跳过未取得Frame的正文和destroy；创建成功仍执行原正文并由原根释放Frame。

普通uniform沿逻辑分量传入。模块资源共同执行入口的每个逻辑标量项使用16字节、对齐8的Value：u32标签在偏移0，最大8字节union载荷在偏移8。标签0至10依次为f32、u32、i32、bool、u64、i64、f64、u8、i8、u16、i16；u8／i8／bool的活动载荷仅1字节，u16／i16仅2字节，u32／i32／f32为4字节，u64／i64／f64为8字节。bool活动字节为0／1，B普通bool的4字节布局保持；填充及非活动union字节不是值。现有[Value源码](../../crates/blcompute/src/lib.rs)使用`#[repr(C,u32)]`，布局依据[Rust枚举C表示规则](https://doc.rust-lang.org/reference/type-layout.html#layout.repr.primitive-c)。生成包装保留原参数调用约定，并将这些值在栈上交给共同运行库，根直接借用到本次同步完成。类型／程序基址仍由链接器静态绑定，同一Frame的句柄、拥有关系、显式upload／download与实际CPU／GPU完成规则保持；此表示不是可跨任意Frame使用的通用资源句柄。


整域共同执行入口的DatasetBinding为八字节、对齐四，handle与记录内字节offset分别为两个u32；它只引用本Frame的原存储。已编译整域函数使用同一根句柄／offset语义，长度和物理stride来自原资源所有者。CPU范围入口的第一参数指向BufferAddress表，每槽十六字节、对齐八，原地址位于偏移零，usize stride位于偏移八；纹理槽只消费地址，范围摘要与排序槽使用实际工作区布局。该表示不替换普通CPU函数的借用签名或语言数据布局，也不是跨Frame的通用资源句柄。

CPU整域调用中的纹理参数继续是原只读BufferAddress列表地址；单项／数组形状、维度、格式及参数次序随原module/domain接口交接。blink_run第五参数为每组列表首地址的连续数组，数量与各组长度来自本次Program的TextureBinding；普通统一值与DatasetBinding保持各自布局。Frame::run与CpuTemplate同步借用实际描述、mip及像素，原调用者和代码所有者保持至全部CPU工作完成，Frame不保存另一份纹理拥有者、登记或释放链。Rust调用者可用cpu::CpuTextures::new借用原TextureInput，再由bindings取得带生命周期的CpuTextureBinding；真实C ABI调用者承担有效地址、布局和整个同步消费者期间的寿命，非空不代替这些责任。GPU图像不能作为CPU描述，GPU整域输入仍待C-003.2.3.1.2.2；B拥有纹理构造亦未由这项只读借用实现。

CPU并行范围可接收普通CPU函数返回的拥有dataset，并在该范围内读取、移动和结束该资源。返回句柄、载荷、静态释放函数及正常／失败出口使用本次范围的同一私有Frame。后续可失败调用失败时，先结束实际仍拥有的范围值，再沿原范围错误结果交回调用者。跨任务转移拥有结果及范围内发起子域仍沿对应未完成能力。

所有CPU范围及集合延续入口使用同一RangeEntry：void* entry(const BufferAddress* buffers, const u32* uniforms, u32 end, u32 begin, u32* output)。五参数次序和Windows x64位置固定，返回空指针成功，非空为同一运行库交回、由调用者唯一拥有的原错误。output按当前StageCode的RangeResult::Values字数持有仅在成功时可采用的算法输出，计数同样写一个u32；None时不解引用output。框架等待全部CPU工作真正结束后沿原错误结果槽消费所有返回值，多个独立错误由原error_join合并后交给原Result；失败跳过结果合成及后续阶段，已经发生的写入不回滚，拥有输入仍按原run_owned错误交回。同步集合前缀／归约尾沿同一原调用者先消费错误再采用输出。已链接范围需要资源时，ExecutionPlan.native_frame给出原uniform块中的隐藏Frame地址偏移，该地址只在同一次入口有效；本次私有Frame引用原不可变定义／代码并独占局部存储，原普通函数F和资源退出完成后才结束。源码闭合SDK映像同时交接真实资源包，由原装入器一次绑定代码与标准C入口；其源码／算法DLL完整交付及独立obj／lib输入分别见C-027.35.4.2.2.3.2.2.2.2.1／.2；源码闭合SDK消费原资源错误并保持宿主存活，已链接算法DLL消费原模块错误，原始SDK的已编译提供者沿同一拥有者归还真实私有分配及F错误，普通拥有结果等范围仍各原编号，不能把未完成的B266用例计作成功。

普通on compute／on gpu函数可借用write或inout dataset，并沿原源码在相应CPU／GPU元素阶段展开。write没有旧内容的读取许可，正常返回须按原initialized合同交出本次实际当前位置及字段；空写、未满足条件的缺写不能形成无条件输出。实际展开保持原当前位置、共享读取及同次独占借用约束，不授予任意非原子散写权限。GPU侧直接使用原元素存储、位置与Stage，不使用CPU的Frame／实际地址参数ABI；完整设备普通调用、递归及其模块支持继续各按真实缺口记录。

普通CPU Invocation函数的read／write／inout／atomic dataset内部借用参数为32字节、对齐8：root:u64位于0、length:u32位于8、stride:u64位于16、offset:u32位于24。root是真实当前存储地址，stride是完整物理记录步长；字段投影保持父记录步长。该具体函数在源码及已编译调用中使用同一ABI；Host dataset参数仍使用真实Frame句柄、长度、u32步长与字段偏移。CPU应用从原Frame取得实际存储地址，CPU范围直接消费BufferAddress；Invocation read／write／inout／atomic使用同一物理描述，权限继续由原Access及实际来源交接。GPU驻留数据允许普通CPU函数只取元数据，载荷读取仍要求CPU存储；GPU地址不授予CPU访问权。原调用Value仅附带实际Frame／handle及对原调用节点的编译引用；源、导入及递归合同由原Module闭合后，同一finalize消费真实写字段与条件，降低到原Frame地址入口再交付原值参数。没有载荷写的元数据调用不取得CPU写地址；已是裸地址的范围及再借用不携带Frame。字段转借在原DatasetView保留本次Access，投影及嵌套转借保持或收窄权限，不改写Stage根的原权限。整份Dataset与实际元素位置的同次借用沿原binding和字段字节区域比较，inout重叠由原B227拒绝；同源read／read、atomic／atomic及不相交字段保持合法。编译引用消费后释放，资源、来源与view的最后消费者规则保持，普通外部C ABI仍不直接接收dataset。范围调用的被调布局前提必须有真实事实，尚不能经域接口交接时报告B270，完整范围见Issues C-027.35.4.2.2.4。普通dataset函数的实际载荷访问沿原Dataset闭合合同交接字段、元素下标、成立条件、真实Read／Write／InOut及atomic操作属性；atomic_load、atomic_store和读改写分别保留实际读写方向。普通赋值、动态数组和嵌套已编译调用沿原Place／Origin及同一合同投影，数组候选保留真实下标选择条件。参数的read／write／inout／atomic许可、CPU驻留和只取length不代替载荷效果。已编译调用的真实写区进入原地址副作用消费者；CPU范围的原memory_regions按实际普通共享读取与写区比较，B166、分离字段和当前位置规则保持，atomic别名仍表达共享操作。原Frame地址入口同时交接每元素写字节、实际stride与count；普通write／inout范围调用的非原子写须代入本次实际下标并对应当前元素，跨元素写由B144拒绝；CPU write dataset的完成义务按本次真实借用位置交接：独立函数保留写前读限制，正常返回把必需输出位置、实际写入及成立条件交给原初始化合同；CPU范围只要求当前实例获准的元素与字段，普通Host调用仍须完成实际借出的整份范围，空集合没有元素义务。调用处不以旧初始化值补足本次write输出，源码与无提供者实现的对象／归档消费同一合同；实际写字节归原Stage.write_regions，由原Map位置／调用效果、atomic实际Place和各collective的目标字段／标量直接交接；原writes绑定列表仍只承担阶段依赖。所属ExecutionPlan只组合本次实际编译单元的阶段范围到原ProgramLayout.write_regions，拆分子域单元时不带入其他单元的写入；不回扫IR、不复制图或新建运行状态。Frame／CpuProgram及GPU提交沿原bindings::invalidate撤回与Count四字节真正重叠的关系，投影叠加实际offset；未写兄弟字段和纯length保持。编译期原prefix::Facts消费本次实际操作范围；源码展开不再按声明重复撤回，已编译子域沿同一导出合同和原DomainCall交接实际参数范围，再按原保证更新事实。原模块合同只持此函数的闭合效果，资源、scope与完成仍各归原所有者。Invocation atomic的真实地址参数由原范围或Frame资源借出，原Frame调用的实际可能写字段沿已闭合Dataset合同交接到原地址入口；inout实例借用沿当前元素与真实字段范围判定，CPU write dataset的完成义务按本次真实借用位置交接：独立函数保留写前读限制，正常返回把必需输出位置、实际写入及成立条件交给原初始化合同；CPU范围只要求当前实例获准的元素与字段，普通Host调用仍须完成实际借出的整份范围，空集合没有元素义务。调用处不以旧初始化值补足本次write输出，源码与无提供者实现的对象／归档消费同一合同。上述编译引用在finalize消费后删除，不进入普通函数机器参数或执行映像。CPU范围的已编译普通函数代码由所属算法或B应用模块COFF交接，原静态链接统一选择真实提供者；有资源的B应用将原代码、运行库和资源绑定一次链接到同一EXE，Frame不重新查找普通函数。CPU范围中明确on cpu的普通源函数沿原native_declaration／native_call和实际参数生成原生Call；Compute共享函数保留原源码内联选择，其CPU递归回边沿同一普通函数编译器交接；同一具体实例由本次原Module持有，不按调用点或阶段复制函数闭包。应用／模块的范围代码和普通callee由所属COFF统一交出，私有实现不成为资源映像的另一权威。独立CPU入口映像持有自己实际消费的命名callee，与范围／集合入口共同装入同一VM，重定位在本映像内绑定；原展开表和外部库在最后真实CPU消费者结束后共同释放。已链接B应用与模块的CPU范围按原调用闭包接入实际Frame。Program／ExecutionPlan只交接原uniform块中隐藏Frame地址的明确偏移，不增加用户业务参数。每次实际范围入口由原RangeWork沿同一Arc<Definitions>引用已绑定代码和类型／program定义，只创建本次私有Frame存储；原执行输入保持实际存储和view资源直到全部消费者结束；子Frame只在原data槽中保存带真实布局、长度及权限的非拥有CpuBorrow，局部分配仍由该Frame拥有。父Frame本身不被并发可变借用，输入载荷不复制、不伪造拥有Dataset，也不增加释放链。范围入口与同步集合入口共用同一原调用者，进入后由普通callee沿原静态拥有退出结束F及局部资源，最后真实入口完成后才结束子Frame；不重解码工厂、复制定义或重建代码VM。原RangeEntry交回空指针成功或唯一拥有的实际错误，算法输出仍写原output；全部线程池工作真正结束后，原execute消费所有错误并沿原error_join及Result交回，原输入及部分效果仍归原Frame。Host dataset沿现行16字节描述交接本次私有Frame中的真实借用槽、长度、u32步长及偏移，Invocation仍沿原32字节实际地址描述；字段投影保持物理stride和访问权限。范围入口在原DomainBindings::require接受接口前提后，直接按本次真实参数从原DomainBindings投影当前实际prefix与字段数值关系，不从requires声明重新签发事实。投影只保留实际参数完整覆盖的目标字段与计数四字节，按本次逻辑槽及字段偏移交接；同一实际冻结存储的重复只读参数复用原身份，未绑定的资源、计数或字段不进入私有范围。原存储、物理stride和最后消费者寿命继续由原CPU输入保持。原Stage持有本次实际生产的prefix_guarantees与write_regions，ExecutionStage保留同一执行效果；进入每一阶段前由原bindings::invalidate按该阶段实际字节写区间撤回，全部原生工作及延续成功后才接入该阶段的新保证。后继RangeWork把当前仍有效关系与同一CPU参数借给私有Frame的原DomainBindings；程序内部生产不提升为根输入requires，未执行／失败阶段不发布保证，未覆盖兄弟字段与真实投影保持。阶段融合按后继写入撤回前序生产，再合并后继生产；不保存阶段事实快照。这里不复制父Frame域历史、不读取或扫描载荷，也不创建第二关系表或释放链；每次私有Frame及其实际输入借用在最后真实消费者结束后结束。普通CPU函数可声明requires prefix(data,count.field)：计数来自独立形状1资源中的可读u32字段，调用点沿已生产的原Count存储事实证明实际数据及投影，函数入口使用同一前提。静态前缀事实属于DataPayload.prefixes，不属于数值Proof，也不授予访问、初始化或资源寿命；复制计数值不带走前提，真实计数字段写／未知写沿原模型撤回，未写兄弟字段保持。条件、循环、拥有移动、返回及普通调用输出沿原Data模型合流与实际ABI参数命名交接；地址与物理布局不作为前缀身份。生成并行域从实际捕获的计数字段和数据投影形成原输入合同，PrefixRelation按domain_region表达实际字段字节区域，完整资源保证可供其投影使用，子区域不提升为整个父资源。原运行期入口仍只消费真实DomainBindings中的有效关系，不从requires签发事实；普通Host接口不开放ensures prefix；整域接口继续交接原有输出保证。CPU范围的普通函数共同元素调用前提沿实际DatasetView查询原Builder.element_facts，按原字段布局与投影选择；原memory_regions只撤回真实覆盖的字段，未写兄弟字段保持。选择保留原共同元素量词作用域，空集没有元素前提，不据此假设独立目标非空；不扫描运行数据、包装实际参数或建立第二事实来源。生成的CPU parallel从实际捕获dataset的原共同模型交接数值范围和逻辑数据目标，直接进入原Program.element_requirements；域编译只按原输入合同建立Facts，普通调用与实际写撤回继续消费同一事实。原已编译域接口保留真正依赖的输入前提，调用处直接消费，不从源声明重复重建；数值编码、类型布局、字节投影和空集合按原语义，写后未再次生产的事实不继续成立。原生Frame沿现有Range／Index输入消费及scope交接，不增加扫描或运行表。Stage与ExecutionStage交接本阶段完整实际写后确立的element_guarantees；融合按后续真实字节写撤回被覆盖的旧保证，再接入后阶段的真实保证，未写兄弟字段保持。需要Frame的CPU执行在本阶段全部原生范围、continuation及资源调用成功完成后，将这些Range及逻辑Index目标沿原转换和同字段交集交给原input_domains，后继私有Frame只消费当时实际成立的事实。整程序最终保证仍在整程序完成交回，不提前用于中间阶段；未推导、失败或未完成的工作不发布新保证。该交接不复制全Facts快照、扫描运行数据或增加完成／状态表，原错误传播及最后消费者生命周期保持。源码闭合SDK将同一次Finalized.resources移交Program，再由CPU ProgramImage共同交接本目录类型、实际子program和命名普通代码；原Allocation一次装入根／子入口与callee，内部标准资源C名绑定同一运行库的真实入口，局部分配、整域执行和字段完成直接形成原C ABI参数与同步栈借用，不新增Rust转发或运行时编译。IR局部位置按分量槽编号，C数组的字节、对齐及填充只由原Ty交给后端。独立CpuCode持有本映像的只读Arc<Definitions>，子program复用同一代码拥有者并由原Frame交接目录，不反向持有根目录；原不可变目录与VM、展开及真实DLL依赖保持到最后消费者结束。ProgramImage的代码捕获和恢复按根阶段、命名callee及同一资源目录中的CPU子映像顺序遍历，GPU不占CPU材料条目；code_json的resource_images按原目录索引查询实际CPU／GPU子映像，不另存代码目录或装入执行资源。原始及缓存映像继续携同源资源包，由原VM装入；静态算法DLL将根目录移入原模块工厂表首项，与实际选中的obj／lib模块沿同一type_base／program_base规则组合。根执行映像只保留阶段、callee及已链接索引，不再嵌入第二资源目录；同一静态入口地址表服务根映像与根工厂，代码由所属PE／DLL持有，不建立执行VM或运行时编译。SDK的compile_with_modules／compile_entry_with_modules由原唯一前端消费obj或lib解出的公开接口、闭合合同及Requirement；object::Input::objects拥有实际读入的模块，编译产物只持本次所需的导入合同，不读取提供者h／b或重建其私有实现。普通应用带模块入口复用同一read_world／compile_world。普通CPU函数及其已闭合资源由原compile_dll／link_inputs消费实际obj或lib，在所属算法DLL一次绑定代码、资源工厂及依赖；提供者h／b缺席不改变这条已编译交接。Definitions::linked按原工厂顺序恢复真实类型及CPU／GPU程序，由实际消费者将CPU入口绑定到所属PE／DLL或同一原始Allocation，GPU保留原映像；普通应用Frame::from_linked_modules与算法根CpuProgram::from_linked_modules消费同一所有者，各范围仍由原from_definitions创建实例私有Frame，借用本次实际输入。没有临时Frame取目录、第二目录、注册表、VM或宿主转发。无需Frame的算法构造继续不引用工厂表。ModuleImage及ModuleFactory的纯C布局声明由resource/module_image.rs唯一提供，原生成器直接展开同一声明，非根元数据工厂不依赖blanguage或blcompute。C字段与代码／字节寿命沿原工厂契约交接；原始CPU SDK的native_with_inputs在根代码生成后消费已读入Input，父范围及源码资源子program的现有导入合同共同交给本次代码闭包，普通Application、算法DLL与原始SDK各自消费真实依赖，不改变各IR的局部imports；由原linker按这些合同选择obj／lib及关联节，原combine交出唯一ModuleCode；根范围不复制为第二份COFF，也不读取提供者实现。ModuleCode保存实际节、标志、符号位置及重定位，原Allocation将其与根范围放入同一次VirtualAlloc，系统IAT与DLL、实际页面权限及.pdata／.xdata展开沿同一拥有者保持，原函数表注销后释放。ADDR64／ADDR32NB／REL32由PE与VM共用原计算，运行时不重新选择符号或编译。原compiler_output_bytes与unsafe同源恢复交接完整材料，新的独立消费者不再需要提供者h／b／obj／lib。已编译资源模块的非根静态侧库明确沿原库目录抽取实际包装／工厂，复用同一COFF物理解码；普通公开B对象继续要求原BLCO合同。原链接队列保留实际资源CPU闭包，将包装代码和真实IAT槽、类型／program基址及工厂表交给同一ModuleCode；地址槽保存函数地址，不把间接引用直接改成代码地址。原始目录在根源码类型／program之后编号，实际资源ABI由宿主运行库提供给原模块绑定；静态成品仍由自身根工厂提供ABI，Definitions只合入同一次解码的真实定义，CPU入口共同持有原Allocation，RangeWork仍拥有各次私有Frame。原始代码与目录随同一映像输出／恢复，不复制静态运行库、重编、生成DLL或重走已完成的符号图。非根参数包装与工厂沿原编译阶段形成，Rust构建依赖保留，装入阶段只消费已编译材料。单入口缓存使用Compiler::build_entry_with_inputs或build::compile_entry_with_inputs消费本次拥有的obj／lib。先从原Object或库成员的B元数据与实际导出借用公开接口，不复制代码／资源归档；根编译复用按原source、实际消费声明、符号合同及Requirement决定。命中时保留原根计划和机器码，当前模块代码／资源仍交给同一原链接器选择与组合；私有实现改变只更新最终链接材料，消费的公开合同改变则重新检查和生成根代码。Compilation.reused只表示根编译复用；完整BLBI始终保存可独立装入的最终BLRI，最终字节未变化时不重写文件。不另存模块代码摘要、链接缓存、IR图或状态水位；build_graph的Entry可通过原Dependency::Unit／Object消费图内或外部实际模块；传递依赖、源码捕获与Compiler预算沿原图交接，图输出保持原Arc<ObjectCompilation>或调用者Object直到最后编译／链接消费者结束。原选择队列只读消费拥有／借用对象，资源导入的最终改写在存活节输出时完成；侧库消费归本次原选择记录，不以改空输入归档作标记。最终映像取得实际存活代码／展开与必要符号／重定位，不深复制整Object或Rust归档，不读回对象文件，不另建图或选择器。GPU原始映像的COFF调用继续未实现。ModuleCode::code_json直接借用所选合并节、实际重定位与原.pdata声明的函数范围，交回原材料、函数记录、确定范围的x64指令及其余未分类字节；ProgramImage::code_json的module_code消费同一结果，无模块为null。节索引从0开始，全部位置相对本节，原字节、符号／加数和实际材料所有者不变；未知、跨节和未绑定位置如实表达，不把节标志、源码名或解码成功当作完整范围、真实装载地址或实际执行。源码闭合SDK沿原OwnedRunResult交回实际资源错误、原输入与部分效果，所有范围及F真实结束后宿主可继续持有原输入并调用同一程序；已链接算法DLL同样在全部实际范围及F结束后交回原资源错误和外部输出的实际状态，宿主可用同一DLL句柄继续正常调用；原始SDK的源缺席obj／lib沿同一RangeEntry及原take_errors归还私有分配／F的真实拥有错误，原输入与实际部分效果继续归宿主；错误消费后可继续使用同一程序；普通拥有结果与内部CPU子域沿本次范围的同一私有Frame及已绑定资源目录交接，内部计算由Frame::run发起，原CPU执行器等待其全部工作完成后才交回存储，范围完成与错误继续由原调用者消费；GPU递归、完整设备普通调用与选择性内联仍按各自未完成能力记录。

已有Frame的共同C ABI中，blink_allocate、blink_resize、blink_run、blink_upload与blink_download直接返回不透明的拥有错误指针：NULL表示成功，非空为调用者唯一拥有的错误。有业务结果时output是第一参数，仅成功初始化；失败不读取或伪造输出。blink_error_message借出原错误的UTF-8文本与长度，调用者结束文本借用后用blink_error_destroy恰好消费一次；blink_error_join消费两个独立错误并交回一个，由原primary分配承载合并文本。错误不借用Frame、不跨C ABI展开；实际指针、类型／程序身份、参数数组及借用范围由可信原生调用者履行。共同入口交回原结果，destroy／release由实际拥有者明确调用；已发生写入和设备故障后的数据可用性沿原执行合同解释。

生成B应用直接消费同一拥有错误：普通B调用的真实失败沿原作用域结束实际拥有者及已接管释放组，再沿普通调用交回唯一错误；只有应用根报告、销毁同一Frame并退出1。非根模块借用原不透明Frame，错误表示、文本及销毁只归原运行库。创建Frame仍在同次创建调用借用错误回调，创建失败不执行正文；普通B的源码Result与这些内部资源错误分别表达。普通实参求值及ABI准备沿同一原env保持已求值拥有者，callee进入前交付；resize实参采用前的拥有交接沿原作用域处理；resize动态上界及其他未完成范围见Issues，完整C声明见[NativeInterop](NativeInterop.md#共同frame-c-abi的错误与所有权)。

共同载荷地址入口为`u32* blink_address(Frame*,u32 handle,u64 byte_offset,u32 write_bytes,u64 stride,u32 count)`，直接返回同一存储根地址。该受信任入口消费编译阶段已交接的存活身份、目标位置、访问权限和借用；CPU解引用必须是CPU位置，冻结存储只读，原生unsafe调用者承担相同前提。地址取得不分配、传输或等待，不形成可失败B调用；真实运行绑定继续在原CpuTemplate／Frame::run入口交接外部输入。write_bytes或count为零表示没有写入元素；其余写区为[byte_offset+i*stride,byte_offset+i*stride+write_bytes)，0<=i<count。单位置stride=0／count=1，跨元素同字段保持原记录间隙，行号、字段与动态数组偏移由原调用位置合成；分散分量分别交接实际宽度。Frame沿原address_written撤回真实相交的Count及共同元素数值／索引关系，未写字段及记录间隙保持，指针仍保持到最后真实消费者结束；存活地址期间原存储不得移动或释放。原Frame::storage_address唯一取得实际CPU／GPU位置，Owned CPU存储从原Vec取得可写原始指针，冻结存储只交出只读来源，GPU位置不授予CPU解引用。

与当前资源ABI不匹配的资源提供者不能与根运行库混用，须重新编译相关资源产物；没有资源的普通对象按自己的对象格式和合同判定，不因无关资源映像改变而失效。兼容身份只随真实C签名、标量及映像编码／含义变化，无关编译器、工具链或私有Frame布局不成为ABI。函数合同、目标平台和外层对象格式仍各自生效；详见Architecture的“模块资源C ABI与已编译映像”。

现有公开算法DLL及C/C++／Rust包装仍用原create／run／destroy接口。上述边界不代表完整版本演化、一般借用返回／资源阶段、任意迁移、跨机或稳定热更新完成；实际实现、Release和原生范围见Issues。

## 模块静态库
多个已编译独立模块可以保存为一个标准COFF模块库；同一模块库用于B编译导入和最终链接。例如，将上节的数学模块归档：

```powershell
.\target\release\bc.exe -c sample/beginner/math.b -I sample --module -o target/modules/math.obj
.\target\release\blink.exe --archive target/modules/math.obj -o target/modules/math.lib
.\target\release\bc.exe -c sample/beginner/modules.b -I sample --import target/modules/math.lib -o target/modules/app.obj
.\target\release\blink.exe target/modules/app.obj target/modules/math.lib -o target/modules/app.exe
.\target\modules\app.exe
```

输出13。`blink --archive <module.obj|library.lib>... -o <library.lib>`保留原成员对象字节，也可组合已有模块库；库不保存待编译实现。标准COFF描述物理容器；Blink模块合同、嵌入执行归档、导入绑定及根工厂数据由原Blink链接阶段解释并形成最终原生输入。对象元数据中的资源前缀、计数、ABI字节与代码名称由B直接读写，运行映像仍由其资源所有者交接。普通C入口的实际调用闭包需要Frame时仍按现行B263交回，资源ABI的根Frame输入和寿命另由真实消费者持有。`--import`读取成员的原公开接口及已编译具体实例，库.h／.b无需分发；应用链接从唯一实际入口沿实际机器符号选择显式模块及库成员，在成员内继续其真实函数节、重定位目标与关联展开节，只把实际可达的原生代码和DLL导入交给原组合所有者。函数内字面量保持，递归／共享辅助函数沿同一节身份闭合。模块资源archive只在本模块分配／执行包装被调用时保留，仅导出实际调用包装及原工厂；资源仍使用原完整模块映像和本地索引，不裁剪或重排映像条目。栈预留继续消费所选对象声明的上界，不等于函数级最小栈需求报告。

未消费的显式模块、未使用库成员及已选模块内未使用函数的缺失依赖、DLL导入或资源包装不会拖入应用，真正消费后仍须有实际定义。显式对象先提供明确符号；显式符号重复、实际抽取成员与明确提供者冲突仍拒绝。库顺序决定首先引入的定义，合同不匹配直接拒绝，不搜索另一个“兼容实现”。原输入格式读取、唯一实际入口与对象种类边界保持，单对象算法DLL仍以其既有对象为根；SDK单对象构建与文件输入交给同一链接所有者。同名成员文件以标准成员偏移区分，函数／模块名称仍区分大小写。CPU／GPU整域、拥有资源及明确导出的泛型实例沿同一合同和Frame交接；归档不增加隐式传输或运行时注册。

无需Frame的B extern `"C"`入口可由`blink --host-archive <module.obj|library.lib>... -o <host.lib>`交给C／C++／Rust原生宿主静态链接。该产物保留原B对象字节，并为实际DLL／函数增加原生COFF导入成员；实际导入槽拥有其COMDAT关联材料，宿主链接只保留真实引用的组，未调用函数不带入DLL，不同DLL的同名函数保留各自身份。宿主库用于原生链接，B编译导入继续使用原--archive模块库；原生IAT成员不伪造B接口。该命令不读取B实现、不编译执行框架、不创建或托管Frame，也不把签名或符号地址当作外部代码／资源寿命保证。

Rust SDK的`blanguage::object::Input::read(...).objects()`、`blanguage::object::archive(...)`、`blanguage::linker::link_inputs(...)`及原生宿主产物入口`blanguage::linker::host_archive(...)`分别复用同一输入、归档和链接所有者。两类归档写出直接使用调用者的原路径，只按实际父目录关系创建目录，沿原Result交回文件I/O结果；不把事先绝对化作为归档写出前置。当前支持Microsoft COFF双链接成员目录中的B模块；成员保留原B对象元数据，资源模块遵守实际资源C ABI与映像版本。一般外部C静态库绑定、符号泛型模板、完整版本依赖及热更新继续开放。

## 独立模块的整域CPU／GPU入口
现有模块编译也支持dataset借用及普通uniform组成的整域函数。库中的普通返回类型声明保持同一语法，例如`void step(inout dataset<Item> items, u32 amount) on compute;`；模块编译交付CPU与GPU的实际代码及入口，应用通过已编译接口调用：

```c
step(items, 3);                 // CPU位置，items实际在CPU
upload(items);
target(gpu, step(items, 7));    // GPU位置，结果仍在GPU
download(items);
```

库先用原`bc -c ... --module`编译，应用用`--import library.obj`，最后由`blink`消费对象。移走库.h／.b后仍可使用；不在链接时生成CPU／GPU代码。on cpu或on gpu的声明只导出对应位置，on compute导出两者；真正不支持的设备能力仍明确报错。开放泛型／const只交付指定`--instance`的具体实现，未使用的开放声明可以保留在接口，不能临时装入库源码补出缺少的实例。

普通CPU包装可调用这些入口并返回拥有资源，仍使用同一Frame。模块接口保留推导参与要求、布局与实际初始化效果，完整输出和筛选输出分别处理；write参数未被完整初始化时不得正常返回。私有实现变化且合同保持时，应用对象可直接重新链接；初始化／布局等合同变化使原调用对象不能绑定。不匹配格式的对象需在编译阶段重建。

纯整域函数现在也可调用这些已编译入口；本地parallel／集合阶段与外部整域调用可以按源顺序交替，源码中的私有包装、有限const实例与静态阶段循环沿原规则展开。调用仍须满足真实目标、访问、形状、requires／ensures及初始化条件；库的公开具体实例必须已经交付。局部优化只作用于封闭阶段，模块边界是原生调用，移走各层库.h／.b后仍可编译消费者并链接。当前单个内核映像不能包含外部对象调用，跨对象组合使用原生模块或B应用入口；此产物种类区别仍报B270，不代表语言重新要求库实现体。

同一物理数据作为多个参数传入时，非重叠字段可以分别read／write／inout借用；真实重叠的区域只允许全部read或全部atomic，普通可写重叠仍拒绝。Frame只取得一份物理存储，在准备CPU参数指针或GPU描述符时投影到各逻辑参数；有效前缀保留原物理根中的具体数据字段与计数字段身份，同根另一个字段不能取得该生产关系，绑定重排与真实完成不丢失字段偏移。阶段依赖对应原实际存储。同步GPU调用等待该次真实完成，结果继续驻留原设备，调用后原所有者仍按原权限使用数据。

纹理／AS在B应用中的交接、一般资源阶段、符号模板、普通设备跨对象值调用及版本演化继续开放。原C/C++／Rust算法接口保持；不能将上述具体范围写成完整C-021.4／C-021.8完成。

## 普通位置借用与contiguous
普通CPU函数可以直接借用合法重排位置；独立模块也不要求为每个调用布局重编库。例如：

```c
void swap(inout uint2 value) on cpu { value = value.yx; }
// uint4 data = uint4(1,2,3,4);
// swap(data.zx); 之后 data 为 uint4(3,2,1,4)。
```

标量／记录／矩阵／数组按声明布局借原地址，向量按逻辑分量借各自原地址。调用、递归、字段与动态数组元素继续使用原存储，不复制后写回；重复独占分量及真正重叠的可写参数仍拒绝。

完整C指针要求对应普通值按类型的字节偏移连续、顺序一致，并满足该类型对齐。包装未知布局的B向量借用时，使用现有requires明确这一前提，.h与.b合同保持一致：

```c
extern "C" void c_scale(inout uint2 value, u32 amount) on cpu
    from "native.dll";
void scale_native(inout uint2 value, u32 amount) on cpu
    requires contiguous(value) {
    c_scale(value, amount);
}
```

对于uint4 data，data.xy和data.zw满足uint2的连续与8字节对齐；data.xz离散，data.yx顺序相反，data.yz虽然相邻但偏移不满足uint2对齐，因此不能交给上述接口。普通B的swap仍可接收这些合法独占位置，C的布局条件不会成为所有B借用的额外要求。

`contiguous(parameter.field)`选择write／inout普通位置参数及其字段／swizzle，不读取载荷；可以只承诺某个子区域。调用点从真实位置满足条件，向量符号位置按当前函数声明的requires承接；递归及多层包装同样写清实际所需前提。它不授予初始化、重叠、原子、资源或生命周期权限，不生成运行时判断。C read之前仍须已初始化，write必须完整交付声明范围；缺少前提、对齐错误或借用未初始化内容分别按原接口／初始化诊断报告。独立库变更该requires后，旧调用对象不能用不同合同绑定。

## CPU 的外部 C ABI 函数

执行库的CPU代码装入及Vulkan装入在每次OS动态库加载成功时就拥有本次引用；符号解析或后续初始化失败会释放该次引用，不撤销其他独立调用者的加载。正常CPU代码将库保持至最后代码消费者结束，Vulkan实例及派生设备保持其入口所在库；错误沿原结果交回，不改走备用库或重复装入。
普通 CPU 函数和 CPU 计算函数 可以直接调用 Windows x64 DLL 的 C 导出。声明通常放在 `.h`，不在 `.b` 再写实现；只有外部声明的模块不需要 `.b` 文件。私有外部声明也可以放在 `.b`：

```c
module external.start;
struct Pair { u32 left; u32 right; };
extern "C" Pair add_values(Pair value, u32 amount) on cpu
    from "blink_cpp.dll";
extern "C" void scale_values(inout Pair value, u32 amount) on cpu
    from "blink_rust.dll";
u32 main() on cpu;
```

对应 `.b` 的主流程取得 `add_values(Pair(3,5),2)`，再 `scale_values(value,3)`，即可输出 `15`、`21`。需要不同导出名称时在库名后写 `as "真实符号"`，省略时使用声明中的函数名。这里的引号属于 ABI 与链接元数据，普通文本值见“文本字面量”。库名和导出名称为非空、非 NUL 的 ASCII 字符串，只支持 `\\` 与 `\"` 转义。完整 C++／Rust 库和部署命令见 [外部调用示例](../../sample/extern/README.md)。

| 形参 | C ABI 含义与调用要求 |
|---|---|
| `T value` | 按值；普通非空数值、POD 结构体、向量、矩阵或固定数组，布局与 B 的字段偏移、对齐和填充一致 |
| `read T value` | 指向已初始化实际位置的只读指针；只读数据、view 的普通元素和临时普通值也可借用 |
| `write T value` | 指向实际独占位置的输出指针；可以传未初始化局部，返回后全部逻辑字段视为已初始化 |
| `inout T value` | 指向已初始化实际独占位置的读写指针 |

返回类型可以是上述普通值或 `void`。u64／i64为64位，对应uint64_t／int64_t及Rust u64／i64；u32、i32、f32 及 bool 都为 32 位；bool 对应 C 的 `uint32_t`／Rust 的 `u32`，不能换成宿主单字节 bool。返回与读取的 bool 按零／非零解码。向量按 B 的 POD 内存布局声明，不使用 C++ 的 `__m128` 或 vectorcall；嵌套结构、数组的宿主布局必须与 B 完全相同，不能仅凭字段名相同判断兼容。

前端保留类型、确定初始化、位置连续性、对齐和重叠借用要求；离散或重排 swizzle 不能形成 C 指针。外部写入使旧值的非零／范围等证明失效，不从声明推测结果。外部函数是显式可观察效果，未使用返回值也会执行；含调用的阶段保持边界，并行实例的调用次序不作保证。

这是可信外部 ABI 声明：声明者负责真实导出、布局、读写权限及调用约定一致，外部代码只在调用期间借用、不得越过对象访问、保留指针或让异常／longjmp 跨边界。未声明initialized条件时，`write` 约定每次正常返回都完整初始化全部逻辑字段，即使另外返回了失败状态。编译器不读取 DLL 机器签名，也不证明外部实现遵守声明。

from导入只允许无正文、具体签名的 `on cpu` 普通函数；可声明requires nonzero的按值输入／字段前提及ensures initialized条件输出，其他requires／ensures仍未开放。dataset、tile、view、纹理、加速结构及含资源的值不能穿过这个 ABI，atomic 权限也不支持；可以借用其 CPU 端已合法取得的普通数据元素。address值及unsafe_position<T>按本规范原生地址章节交接；自有地址的完整初始化结果与寿命交接、可保留回调的寿命协议、变参、C++ 类／模板 ABI、静态对象链接与其他平台仍待实现；显式执行上下文及直接B C入口见下节。已装入的外部声明在应用、模块及源码SDK共同的装入阶段检查签名和普通值ABI，即使未调用或已有可复用对象也不能忽略非法类型；声明错误报 B263，未知类型沿原B031报告，实际位置布局不符报 B264；GPU／compute 调用报 B265，不能调用宿主 DLL。已有 CPU 计算函数 与生成的算法 DLL 同样使用此调用实现，普通程序的资源所有者仍属于 blcompute。

独立应用按实际调用在 PE 导入表记录库与符号，由 Windows 装载；嵌入式 CPU 执行使用 LoadLibraryW／GetProcAddress，错误保留库名、符号及操作系统错误。原生代码所有者持有对应模块到最后一个代码消费者释放。DLL 或符号缺失即失败，不替换目标或吞错。应用启动、寄存器、栈参数与结构体返回遵循 [Windows x64 调用约定](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention?view=msvc-170)。

## B定义的原生C入口

`extern "C"`不带`from`时声明由B实现的Windows x64 C ABI函数；`.h`与`.b`保持相同C标记、类型／const形参、参数、目标及输入／输出合同。B定义可以是泛型模板，实际C入口必须是已经实例化的具体签名；普通C值、void／never结果和read／write／inout沿上节真实C布局，只在on cpu普通函数中使用，输入可声明requires nonzero，输出可声明ensures initialized。from导入代表固定DLL符号，不声明泛型模板。它控制调用ABI，符号仍属于原B模块，不自动成为同名PE导出。

以下用最小函数说明C入口取址、签名值与原生调用的交接。普通B内部直接调用add_seven即可，不要求先转成C函数地址。`.h`声明`extern "C" u32 add_seven(u32 value) on cpu;`与原main声明，`.b`可实现：

```b
extern "C" u32 add_seven(u32 value) on cpu { return value + 7; }

u32 main() on cpu {
    cfn<u32,u32> callback = c_function(add_seven);
    println(callback(35));
    return 0;
}
```

两份文件仍写匹配的module声明。c_function接受可见的显式C ABI声明／定义或既有静态函数引用Name<T,...,N>，例如c_function(identity<u64>)、c_function(adjust<u64,7>)。取址没有运行实参，泛型／const须显式给出，不能靠接收变量的cfn类型猜实例；原specialize按调用者可见性、类型集合及u32 const约束绑定，产生原具体实例符号。实际参数、结果及输入／输出合同完成原类型／const替换后形成cfn，条件形参名按argument0起的位置进入签名。普通命名调用继续沿原实参推导。模板的requires nonzero(N)由原u32 const实例化在编译期检查并消费，不成为C调用者的隐藏运行前提。推导的let函数值与显式cfn声明共享实际签名和write／inout位置分析，不要求为位置写入额外补类型注解。独立模块用既有--instance导出需要的开放模板实例，调用方只取该已编译符号；链接不读源码或生成模板。取得地址不执行函数，原符号解析成功后该值非零，可直接沿原间接调用或准确的外部参数交接；普通B函数不自动改变ABI。from声明直接交接已绑定的ExternalName，由原DLL装入／COFF导入及其代码所有者提供地址，不生产B函数体或转发器；B定义继续沿原函数符号取得同一代码地址。取得地址不延长DLL或代码的寿命，最后消费者的保留关系继续按原接口约定。

C位置参数是一个满足T连续布局／对齐的原指针，包含向量时也如此；普通B位置的分量地址ABI保持。C调用从真实位置满足这一条件，不复制／写回以模拟连续位置。B正文按原初始化、只读与独占规则检查，条件输出不伪装为无条件写出。

本地源码普通B调用暂存内部Frame参数，完成真实需求闭包后移除无需的参数；已编译导入直接消费闭合合同的真实ABI，不插入或重复裁剪Frame参数。C入口直接采用同一函数代码，没有隐藏Frame或转发器；其完整调用闭包必须无需Frame。直接分配B资源、发起CPU／GPU程序或经普通B辅助函数引入Frame需求的C入口均报B263，不能通过某个普通位置、原生地址或函数签名自动取得根运行库。

源语言不公开可复制、返回或装入普通记录的Frame身份值，也没有取得当前Frame的内建入口。根运行库的资源C ABI及内部Frame传递仍由其实际所有者使用；普通C ABI不接收dataset或拥有资源，明确外部地址不产生Frame权限。需要B资源的可保留回调继续沿C-021.6.1.3设计真实代码、状态、访问及最后消费者关系，当前不作为已有能力提供，不另建Frame或由宿主转发资源操作。

bc在编译阶段生成函数及符号地址，blink只解析原COFF／归档重定位；提供者源码无需进入对象消费者。保存或复制cfn不延长模块代码和外部状态寿命。注册方必须保持真实代码／状态到注销和最后回调结束，普通C位置只在本次调用内借用；完整可保留关系及动态卸载／热更新沿C-021.6.1.3继续，不能据静态入口宣称已实现。

## 数据能力和编译期检查

| 语义 | 当前行为 |
|---|---|
| read dataset | 共享读取；不能写入，也不能在调用时提升为 inout |
| write dataset | 本作用域输入旧内容不可读；实际初始化字段可读，完整域和字段完成后可转借read／inout |
| inout dataset | 计算函数 中读写当前独占元素；CPU 应用可将整份已初始化存储独占借给普通 CPU 函数 |
| atomic dataset | 通过显式u32／i32／u64／i64原子操作共享修改；普通元素读写被拒绝 |
| tile_atomic／atomic tile<T> | 同一逻辑块内的原子共享值及协作借用；不产生全局散写权限 |
| tile_barrier() | 根块内整块同步；分支、循环、普通值函数内拒绝 |
| clear_prefix／append | 建立空输出前缀并有界接纳完整记录；不开放裸票号或任意散写 |
| parallel 元素域 | 当前 index 与域绑定，不允许未证明安全的任意散写 |
| 固定形状 dataset<T,N> | 编译器可对索引范围证明；设备绑定时检查真实形状 |
| `.h`／`.b` | 模块、数据、签名、访问能力与设备目标一致性检查 |
| 不支持的语义 | 源位置诊断，编译失败；不会改成另一种算法执行 |

跨元素的可变读取会形成依赖，不能当作独立 map 无条件融合。不同元素域、不同设备能力和访问能力在编译期处理；实际 DLL ABI、资源归属、形状与设备能力仍属于必须检查的原生系统边界。

已实际触发的边界诊断包括只读写入 `B144`、接口不一致 `B028`、固定形状越界 `B137`。这些能力不等于证明全部算法：浮点定义域、数值稳定性、动态循环终止和业务正确性仍须由程序设计负责。

## never 函数结果与非返回路径

`never` 作为普通函数结果，声明执行不会通过正常返回把控制交还调用者。函数体的每条实际路径都须保持这个保证；执行到函数结尾或执行普通 return 均报错。已知无限循环、调用另一 never 函数以及这些路径的条件组合沿原 Flow／返回分析处理。普通返回类型的函数也可以具有不返回的路径；只有真实正常返回边需要交付声明值或完成 write 输出。

| 函数结果 | 正常返回时的含义 |
|---|---|
| u32、记录、dataset、result 等 | 交付声明的普通值或拥有资源 |
| void | 正常返回，没有普通值 |
| never | 没有正常返回边，不提供值或 Unit |

```c
extern "C" never terminate(u32 code) on cpu
    from "kernel32.dll" as "ExitProcess";
never stop(u32 code) on cpu { return terminate(code); }
u32 choose(bool ok) on cpu { return ok ? 7 : stop(2); }
```

声明为 never 的调用沿原实参顺序求值；调用发生后，当前路径没有继续值，后续实参、外层运算、赋值和普通返回均不发生。因此选择式或短路式可以把不返回的分支与正常值分支组合；未选中的不返回分支不会执行。语句的已知不可达分支继续沿原规则消除。普通 void 调用仍不能给需要值的表达式提供值。

普通原生调用、静态内联和已编译模块接口保持相同的结果合同。没有正常返回时，不交付调用后的写入事实、输出初始化或临时拥有参数的正常归还，也不制造一个空返回补齐返回表；必要存储仍按真实执行与所有者存活。never 可能表现为进程退出或持续执行，不表示工作成功、设备完成或资源已经归还。

外部 C 声明中的 never 是调用方提供的真实 ABI 合同，与现有外部声明一样信任实现；不按 DLL／符号名称猜测，也不插入运行时检查。把 ExitProcess 声明为普通 void 时，调用点仍只知道它可以正常返回。私有 ABI 没有普通返回载荷，现有应用入口仍按 u32／i32 交付进程退出码；main 的函数体可以通过 never 调用结束全部路径。

never当前用于函数结果及相应控制流，不分配普通返回载荷。普通变量、数据参数、记录字段、result的never载荷等一般组合仍未实现。

## 普通位置的复合赋值

普通赋值语句支持 `=、+=、-=、*=、/=、%=、&=、|=、^=、<<=、>>=`。复合赋值沿对应二元运算的类型、标量广播、整数范围与写入规则处理；`%=、&=、|=、^=、<<=、>>=` 使用 u32／i32 标量或向量，不将 bool 或浮点数当成整数位值。

先取得左侧实际位置一次，再求右侧表达式；右侧继续后，读取该位置当时的值，执行对应二元运算并写回。字段、唯一 swizzle、固定数组分量和合法 dataset 位置沿原位置规则处理，左侧的索引／调用不会为了读改写再次求值。右侧如果改变同一位置，运算读取其改变后的当前值；如果通过 try 提前退出，则没有本次复合赋值的后续读取／写入，已经发生的求值效果保持。

```c
void pack(inout u32 word, u32 octet, u32 shift) on cpu {
    word &= ~(255 << shift);
    word |= (octet & 255) << shift;
}
```

取余仍要求每个除数分量已证明非零，结果沿既有整数除余语义；移位量沿既有32位整数的低五位语义，u32 右移为逻辑移位，i32 右移为算术移位。复合赋值需要先读取旧内容，不使未初始化位置、只读位置或普通写入不可触及的原子位置变得可用；它本身不形成原子操作。

这些形式只构成语句，不提供赋值表达式或链式赋值。`<<=`／`>>=` 各是连续的单个 token，不把 `< < =` 或 `> > =` 等分开的符号拼接成运算符；原来的移位、比较及嵌套类型闭合仍按自身语法解析。

## 数值极值与显式 clamp

`min(a,b)`／`max(a,b)` 支持 u32、i32、f32 标量及相应的 2／3／4 分量向量，按分量取最小／最大值。u32 按无符号顺序，i32 按有符号顺序；整数结果保持原精确位值，没有先转换成浮点数。两个参数可以是同宽向量，或让单个标量广播到另一参数的宽度。

已定型的数值参数保持同一基础类型，不隐式混用 u32／i32 或整数／浮点；整数内建的未定型整数字面量沿普通二元运算的现有类型规则确定。bool、枚举、记录、数组和不相容的向量宽度不属于这些数值重载。f32 运算保留原浮点语义，NaN和负零遵守该算子的数值语义。

`clamp(value,lower,upper)` 依次取得三个实参，再计算 `min(max(value,lower),upper)`，接受上述类型与逐分量广播规则。所有实参从左到右只求值一次，实际副作用保持；某个实参通过 try 提前退出时，后续实参和运算不发生。整数上下界反向时，按该表达式得到 upper；不新增上下界检查或隐式失败路径。

整数极值复用已有范围、非零、索引域及同值来源事实。例如资源非空时，下面的显式取较小值可以产生合法索引：

```c
if (rows.length > 0) {
    u32 slot = min(candidate, rows.length - 1);
    println(rows[slot]);
}
```

这来自结果不大于已经合法的索引；`min(candidate,rows.length)` 仍可能等于长度，不能冒充合法索引。max 只有在两个输入均满足同一索引域时才能沿这项关系取得索引事实；零长度、真实越界、混合类型及零除数仍由原语义报告。整数常量和已证明的固定结果沿原常量机制处理，不增加运行时值镜像或额外校验。

### 完整parallel的共同字段数值范围

一个无prefix／where的完整parallel结束后，各元素共同成立的字段数值范围可交给后续同域游标。前一阶段将每条记录的level写为index()%2，后一阶段可用cell.level索引长度二的原子计数；显式phase保留两个实际阶段时仍成立。整数、有限浮点、向量分量和普通分支沿原Proof区间合流，不继承前一阶段的寄存器、条件或借用身份。

源码展开与独立obj／标准lib消费同一完成效果。已推导的共同数值集合由原Interface.elements编码；提供者实现源码不在消费根时，已编译整域及转导出链仍交回该结果。原write_regions按实际字段字节撤回旧值并交接新范围，其他字段保持。未知输入、未知覆盖及未闭合写值的原子操作不提供新的共同值域；普通稀疏／有效前缀的未覆盖元素和append保持原真实覆盖语义。部分原子更新只有在旧共同值与本次所有可能写值合流后仍能成立时，才交接该共同范围，不能以某次写替代未写位置；原B137按真实前提保留。

普通B应用的target调用将结果交回同一Data.elements，独立模块与本地源码共用该合同；字段投影仍指向实际原存储。元素读取按当前Place交接共同数值范围和当前元素事实的交集，不让游标局部未知Proof挡住共同结果，也不复制游标范围快照。完整数值输出不授予来源、借用、地址、散写或资源寿命；初始化、权限、驻留与完成各有原职责。普通局部值读取保持原规则。共同元素内容保持自己的作用域身份，不把模型槽号当作外层寄存器的数值来源。

这项能力交接原生产者已经推导的共同数值集合，不把未知内容或所有输入相关符号关系补成事实。持久／外部字段的输入与目标关系合同仍见Issues C-021.1.4.5.2；普通CPU dataset顺序遍历的直接字段赋值，按已经形成的真实游标位置投影循环写；只写other时，level及独立资源的共同事实保持，实际相交字段和未知写仍失效。字段投影、宽字段布局、拥有记录中的dataset及独立对象／归档沿同一规则使用。普通数值字段沿原数值写入路径；命名拥有元素游标可沿真实位置替换子dataset或拥有聚合字段，直接构造及普通函数返回值均按资源赋值交接。实际左侧位置先求值一次，再按其类型消费原元素模型、覆盖及最后释放；只读入口及仍被view借用的旧资源不能替换。普通前向元素游标可在对当前字段的直接或普通引用写之前读取本次原输入数值；该输入属于当前绑定，不能当作整份资源在循环中仍保持旧共同值。循环头只撤回先前轮次可能影响该新位置的输入，当前语句的真正写仍更新／失效相同绑定；跨元素、重访、写后读取、seek和真实未知写不取得过期输入。跨调用数值合同及未来Memory精度的剩余分别见C-021.1.4.5.4.2.2／C-021.1.4.5.4.3.2.2。对象格式保存当前整域输出合同，旧模块对象需重编；执行映像格式和C参数不因本数值交接改变。

### 有限浮点结果到整数索引

从B内已知整数值域转换得到的有限浮点值，经过min／max及clamp后保留实际数值界；向量、字段、选择、局部赋值及普通B参数／返回复用同一Proof区间集合。原值是u32时，u32(clamp(f32(value),0.0,15.0))的结果可证明在0至15，原索引检查可用于长度16的读取或原子访问。仅有f32类型、普通宿主字节、外部C返回或任意浮点纹理不自动提供有限性；覆盖后的新值按真实来源重新分析。

共同范围推导当前覆盖F32／F64加减乘及各自目标参考模型分母域内的除法，以及已知有限输入的min／max、abs、floor；正有限输入的log／log2交接有限结果，不预测精确设备对数。加减由同一主值和低位残差形成有向端点，再向实际Scalar外舍入；乘法沿同一四角，F32端点积在F64中精确，F64上下界复用原有向乘积及符号对称关系。除法解释[Vulkan精度规则](https://docs.vulkan.org/spec/latest/appendices/spirvenv.html#precision-and-operation-of-spir-v-instructions)时，F64采用[Khronos双精度一致性模型](https://github.com/KhronosGroup/VK-GL-CTS/blob/main/external/vulkancts/modules/vulkan/shaderexecutor/vktShaderBuiltinPrecisionTests.cpp)对跨精度规则的具体解释：Div沿实际FloatFormat给出2.5 ULP及[2^MINEXP,2^(MAXEXP−1)]，double实际使用MINEXP=−1022、MAXEXP=1023、52位小数；附录表没有独立64位列，不将其冒称IEEE除法正确舍入。F64在同号非零的有限分母包络绝对值[2^-1022,2^1022]内，对四角商以4×max(|q|×2^-52,2^-1074)向上形成误差并沿原有向加减外扩，覆盖目标误差及宿主端点求商。F32保持原[2^-126,2^126]及原误差，未被扩大。原区间包含两种零和允许次正规归零；可能零分母、非有限、溢出或模型域外保持未知，不成为合法输入限制。原SPIR-V四则的NoContraction、设备浮点模式及机器指令继续生效，不新增运行检查、保护或替代除法；更完整严格浮点沿原能力范围推进。

同一动态无符号整数尺寸与坐标的关联由原Proof数值域持有：F32转换记录实际整数快照、类型及有效数值域，余数／已成立Upper关系形成相对上界a*d+b+c/d（a非负）。非负加法、非负常量缩放、与同一尺寸转换的除法及同基值仿射／倒数乘法按实际F32舍入交接该上界，原绝对区间继续持有下界和其余结果；正d域只需端点求最大。转换精度边界、除法误差、次正规值及中间溢出共同解释，不能从理想商的非负性推定零附近GPU结果可直接转成无符号索引。原函数合同按实际实参身份和数值域替换，私有或改写后的来源不混同；对象／归档保存同一事实。该分析不改B算法，不添加运行检查、范围表、表达式图或第二求解器，也不把当前可表达上界形状作为语言能力上限。

NaN、无穷、可能为零或超出该除法保证范围的分母、非法对数输入及不能证明可表示的整数转换不取得新的有限范围。clamp上界等于容量、浮点舍入到目标整数界外、未知分支或真实后续写入仍不能冒充有效索引。浮点比较继续保存真实谓词，NaN的否定不等于有序补集，不套用整数恒等式。有序位只用于编译期区间，运行值布局及指令保持；普通对象／归档按对象格式交接同一范围语义，现行执行映像资源ABI不因该事实变化而改变。

## 模块级编译期常量与命名形状

`const 类型 名称 = 表达式;` 在编译期得到完整值。公开常量直接定义在 .h，.b 只能使用或定义本模块私有常量；同一值只定义一次，不在 .h 声明后又于 .b 重复定义。常量与函数共享模块值名称空间，支持直接 import、限定名和现有私有可见性规则；普通局部或参数可以在自己的词法作用域遮蔽未限定的常量名。

完整 `.h` 示例：

```c
module fixed_work;
const u32 Width = 17;
const u32 Height = 5;
const u32 Count = Width * Height;
const uint4 Delta = uint4(3, 7, 9, 13);
struct Row { uint4 value; u32 audit; };
void blocks(inout dataset<Row, Count> rows) on compute;
void add(inout dataset<Row, Count> rows, uint4 delta) on compute;
void run(inout dataset<Row, Count> rows) on compute;
```

对应 `.b`：

```c
module fixed_work;
void blocks(inout dataset<Row, Count> rows) on compute {
    parallel (row in rows) {
        let values = tile_input(rows.value, Width);
        uint4 total = tile_reduce_sum(values);
        row.value = total + Delta;
    }
}
void add(inout dataset<Row, Count> rows, uint4 delta) on compute {
    parallel (row in rows) { row.value += delta; }
}
void run(inout dataset<Row, Count> rows) on compute {
    blocks(rows);
    add(rows, Delta);
}
```

Count 在编译期为 85，Width 为 17；固定形状和块宽度仍按各自既有语义使用。数据集形状接受可完全编译期求值的u32表达式，包括字面量、模块const／限定名及当前实例的const参数，例如`dataset<Row,(IC+255)/256>`。表达式使用普通运算、类型转换及原常量折叠规则；先代入具体类型／const实参，再形成唯一实际count和原资源布局。头文件与实现仍按原接口规则检查，不从实际数据集容量反解一般符号形状方程。块宽度位置使用普通表达式，能使用已求值的 u32 常量；0 或不一致宽度仍按原诊断拒绝。

const 可保存 bool、已有数值标量／向量，以及不含 view 的普通或具体泛型记录；值类型不发生擦除。初始化复用已有表达式、构造、字段、swizzle、按实际整数位宽的模算术／比较／位运算／除余、已知条件选择和类型转换。整数移位量按原类型取低位，MIN/−1 保持同位宽模结果；已知类型转换和一元运算直接生成原 Constant IR。f32 加减乘除沿同一求值器逐操作按 f32 舍入，结果以原 32 位模式交接，不经 f64 中转、不重排或融合。向量按分量求值，合法的标量广播及普通记录构造沿原类型规则完成；普通值函数在完全求值时可组合这些运算。例如 Unit=1.0/16777216.0、Gamma=7.0*Unit/(1.0-7.0*Unit) 可直接声明为模块 f32 const，不生成 CPU／GPU 初始化。该编译期舍入不保证含运行期近似运算的 GPU 表达式逐位相同。f32／f64 常量比较沿同一求值器采用实际 IEEE 关系；sqrt／sin 等一般数学函数仍未进入编译期求值子集。

声明类型沿用普通绑定／返回值的严格类型匹配：正整数字面量默认 u32，`const i32 N=i32(17);` 显式转换；已经定型的常量名称不因值已知就获得隐式转换或向量广播。不可变命名值仍可作为非零除数、范围和条件分析的输入。局部赋值不能改写 const，资源句柄、含 view 的记录与 void 不能作为 const 值。

普通／泛型值函数可参与初始化，但只有在现有分析能完全得到值、且不留下运行期运算或控制流时才接受。例如 `u32 triple(u32 x){return x*3;}` 用常量实参可求值；函数内部尚未求值的循环即便最后返回字面量也拒绝。编译器不执行宿主程序、设备程序或编译期副作用，不将尚未支持的初始化变成运行期初始化。常量互相引用及经普通函数间接形成的循环沿原调用栈报 B160。

设备整域函数在parallel域外可用普通局部绑定声明完全编译期的普通值，例如`u32 width=W;`，其中W为已特化的const参数。初始化、声明类型及名义身份复用原表达式／构造／常量函数分析，常量按原类型及实际位宽进入阶段环境，不产生宿主求值或额外运行阶段；固定数组、记录及Result等普通值继续遵守自身构造和类型规则。静态if分支和for迭代中的绑定仅在该词法块存活，不能覆盖已有参数／局部，离开块后不泄漏；编译期分支之后的阶段仍沿真实输出初始化与资源关系交接。

阶段参数可以直接传入const名称，包括向量、普通记录、具体泛型记录和bool；阶段派生实参及局部初始化共用同一常量语义。普通常量值不留下运行期指令或副作用时接受，例如`add(rows,round+1)`、`add(rows,uint4(1,2,3,4))`，逐元素消费只生成所需常量。直接传递已有运行期参数仍按原资源／uniform身份处理；当前阶段局部初始化和派生表达式不能求值动态uniform或副作用，报B114，运行期运算放在parallel或普通CPU函数中。普通CPU局部声明与动态控制保持原语义，不要求它们改成const。const不是函数，`Delta()`报B234。

初始化不能完全求值或包含资源报 B234；形状常量未知／不明确或不是 u32 标量报 B015。类型、构造、除零、重复名称、可见性与非法赋值沿原诊断。全部声明包括未使用的私有常量都在编译期检查，没有动态初始化、全局可写存储、GPU 数据缓冲、参数镜像或初始化阶段。

当前引擎渲染由 BLRenderTarget 的 W／H／S 及其真实资源数量专门化 `frame<N,W,H,S,DC,IC,Segments>`，trace 的寻址与投影、resolve 的采样及固定形状使用同一具体实例；旧 render.dimensions 已删除。DC=W*H、IC=W*H*S*S 由引擎资源所有者计算，当前语言形状尚不能直接声明符号乘积等式，不能把这项关系说成完整编译期符号证明。BLEditor 消费者继续明确使用输出 3840×2160、内部 7680×4320；语言与平台层不固定某个应用分辨率。

模块 const 与函数 const 参数是不同职责；固定数组和有限／开放 u32 const 参数已实现，一般数学函数求值与开放式编译期解释器仍未实现。

## 编译期阶段范围循环
program阶段层支持`for(counter in first..last)`。范围接受u32字面量、模块const、具体泛型值、当前静态外层计数器及原常量分析可完全求值的派生表达式，例如0..R+1；沿原阶段实参及表达式检查，不运行宿主函数解释循环。例如：

```c
void iterate<const R:u32>(inout dataset<Item> items) on compute {
    parallel(item in items){item.value=index();item.keep=index()%7==2;}
    for(round in 0..R){
        parallel(item in where(items,items.keep)){
            item.value=item.value*31+round;
        }
    }
}
```

编译器按左闭右开范围展开已有parallel、集合、子program、phase及嵌套静态循环。counter是当前词法作用域的不可变u32编译期值，结束后不泄漏；不能覆盖现有参数或计数器。各轮依次消费真实数据和原requires／ensures关系，已改写的Count关系继续失效，不复制一份宿主权威状态。phase跨轮保留，融合仍受既有域、选择及读写依赖约束。

空范围不展开正文；整个program可以没有执行阶段，原数据及有效域关系沿既有接口交接。展开阶段数量及编译资源随实际迭代数增加，不表示已经有设备驱动的动态阶段循环。普通parallel正文内的for／while保持原运行期控制流；阶段层已解析根为动态或非u32时报B251，包含运行期运算或副作用的派生范围沿B114拒绝；计数器改写沿B134，重复名称沿B125，跨program递归沿B105拒绝。

### 编译期阶段条件
计算函数的阶段层现支持`if (编译期bool) ... else ...`，条件沿原阶段环境和常量表达式求值，允许使用具体const值参数、静态for计数器及其派生比较。只展开实际分支，阶段资源绑定、访问、prefix事实与显式phase继续沿原数据流；不生成运行期CPU／GPU分支或复制数据。泛型值参数仍按当前u32规则声明，例如`const P:u32`后判断`P%2==0`，不能据此推导bool泛型值参数已经支持。

阶段if可以嵌套并出现在静态for内，例如双缓冲算法按`pass%2`选择读写方向。阶段层的`return;`退出当前计算函数，包括从所选分支或静态循环内部返回；子函数返回后调用者继续。非bool或直接动态条件报B251，包含运行期派生运算的条件仍可能由原常量分析报B114；动态控制继续放在parallel或CPU普通主流程。CPU普通if及parallel内if保持原运行期语义。

## 受约束的数值泛型函数

普通函数与 cooperative 函数支持 `T function<T: floating>(T value)` 形式，类型参数放在函数名之后，约束是编译器已知的有限类型集合。`.h/.b` 的类型参数名、顺序和约束必须相同。调用仍写普通函数名，从实际参数推导具体类型；同一 T 在多个参数中必须相同，不为推导而隐式改变数值类型或向量宽度。

| 类型集合 | 允许类型 |
|---|---|
| floating | f32、float2、float3、float4、f64 |
| integer | u32、uint2、uint3、uint4、i32、int2、int3、int4、u64、i64、u8、i8、u16、i16 |
| numeric | floating 与 integer 的全部19种类型 |
| scalar_numeric | f32、u32、i32、u64、i64、f64、u8、i8、u16、i16 |
| scalar_integer | u32、i32、u64、i64、u8、i8、u16、i16 |

自动推导从实际参数类型取得类型参数，也可来自 dataset、tile 或 view 的元素类型，以及 write／inout／atomic 借用。只出现在返回类型或完全不出现在形参中的类型参数需要显式指定；不根据预期返回类型或运行数据猜测。bool、用户记录、acceleration 和资源句柄本身不属于这些集合；例如 view<uint3> 可匹配 view<T> 并推导 T=uint3，不能作为裸 T 的 numeric 实参。内建view<T>构造形成的局部值及其词法类型预分析保留同一完整view元素类型，参与普通泛型推导；构造的范围、位置、权限与来源仍在实际构造处检查。

例如 `.h`：

```c
module generic_motion;
struct BLBody { float3 position; float3 velocity; u32 audit; };
void advance<T: floating>(inout T position, T velocity, f32 dt) on compute;
void step(inout dataset<BLBody> bodies, f32 dt) on compute;
```

对应 `.b`：

```c
module generic_motion;
void advance<T: floating>(inout T position, T velocity, f32 dt) on compute {
    position += velocity * dt;
}
void step(inout dataset<BLBody> bodies, f32 dt) on compute {
    parallel (body in bodies) {
        advance(body.position, body.velocity, dt);
    }
}
```

该调用推导 T=float3；同一 advance 函数也能用于 f32、float2 和 float4。类型名 T 可用于返回、参数、无初值局部和普通局部类型，也可写 `T(0)` 等构造，复用具体类型原有的显式转换／分量构造语义。多类型参数写作 `T f<T: numeric, I: scalar_integer>(T value, I flag)`，分别从实际参数推导。

数值泛型保留已有资源权限、形状、索引、requires、输出完整性和借用别名规则。只读／原子 dataset、不可变 view、独占／原子字段及 cooperative tile 都绑定原实际来源。块内宽度仍从调用方继承，泛型不引入第二份存储或隐式屏障。dataset原子接受u32／i32／u64／i64，tile仍只接受u32／i32；泛型接口须声明目标实际支持的有限类型集合。把T约束到仍含不支持成员的numeric或scalar_integer，不是完整有效的原子接口。归约／扫描值函数也可为数值泛型；无元素参数的单位元可以通过 `bind(identity, uint3(0))` 等统一普通值参数推导，也可声明没有元素参数的 identity<T>()，但普通调用必须显式指定类型；集合回调可使用显式泛型函数引用，现有 bind 继续表达真实的统一普通值参数；详见“静态泛型算法参数”。

编译器在模块加载后从原声明生成每个泛型的所有允许类型组合，按原入口检查函数体；即使某种类型未被 计算函数 调用，也必须合法。例如 `T f<T: numeric>(T x) { return sqrt(x); }` 在整数实例上报 B154，应把约束改为 floating。错误会说明正在检查的具体泛型实例。这里只做编译期类型展开，逐元素泛型函数按实际调用类型内联到 计算函数 的原 IR，只为被调用的计算生成原生 x64／SPIR-V；不产生类型擦除、运行期类型分派、宿主每元素回调或独立执行框架。

声明／推导／类型集合错误报 B231；签名不一致沿用 B028，重复参数类型不符沿用 B124，权限、形状、输出与协作错误沿用原诊断。直接把 cooperative 当普通函数调用仍拒绝，其 tile 实参先沿原借用入口识别，不再被误作普通数值。

显式类型约束支持内置数值集合和下述用户定义的有限类型集合；不写集合约束的开放类型形参见“开放普通值类型形参”。有限 u32 常量参数与显式函数／计算函数 实参见下节；记录 const 参数见下节；开放编译期接口约束、关联类型及静态重载尚未实现。它们发展静态组合，不以运行期类型对象、反射或对象派发补齐泛型。多个类型参数按集合笛卡尔积检查，编译期分析量随组合数增加，尚无通用符号泛型体分析。编译成功不证明浮点定义域、循环终止或业务算法正确。

## 受约束的泛型记录

记录可以声明相同的有限数值类型集合，例如 `struct Pair<T: numeric> { T left; T right; };`。使用处显式指定 `Pair<uint3>`、`Pair<f32>` 等具体类型；函数参数可以写 Pair<T>，编译器从实际记录的名义类型参数推导 T。结构体本身不做运行期类型分派。

完整 `.h` 示例：

```c
module generic_particles;
struct Particle<T: floating> {
    T position;
    T velocity;
    u32 id;
};
void drift<T: floating>(inout Particle<T> particle, f32 dt) on compute;
void step(inout dataset<Particle<float3>> particles, f32 dt) on compute;
```

对应 `.b`：

```c
module generic_particles;
void drift<T: floating>(inout Particle<T> particle, f32 dt) on compute {
    particle.position += particle.velocity * dt;
}
void step(inout dataset<Particle<float3>> particles, f32 dt) on compute {
    parallel (particle in particles) {
        drift(particle, dt);
    }
}
```

泛型记录也可出现在返回、普通值、write／inout／atomic 位置和合法的 dataset／tile／view 元素中。`view<Pair<T>>`、`tile<Pair<T>>` 与 `dataset<Pair<T>, 8>` 按原资源语义借用，拥有 view 字段的记录仍由真实根／子资源图维持寿命；包含 view 的 tile 或原子字段借用继续拒绝。view<view<T>>不开放。普通CPU的含view dataset元素按静态F的真实值及来源构造；view与同一元素拥有资源的组合可通过记录字段表达，长度为零时没有子借用消费者。

记录构造显式写类型实参：`Pair<uint3>(uint3(1), uint3(2))`，按声明顺序提供每个字段，沿原字段类型检查，不按同尺寸数据做转换。函数体里可写 `Pair<T>(left, right)`，编译器在类型展开时替换 T；构造参数按源码顺序求值，函数写效果仍由原位置负责。`Pair<T> local;` 支持原有确定初始化，构造／复制只写真实分量，ABI 填充不从来源复制到目标。

记录字段可以嵌套另一泛型记录，例如 `struct Wrap<T: numeric> { Pair<T> pair; u32 marker; };`。多个类型参数分别指定集合，例如 `struct Entry<T: numeric, I: scalar_integer> { T value; I key; };`，使用 Entry<float3,u32>。每个类型实参必须属于声明的集合；Pair<Pair<f32>> 不满足 numeric，但以用户集合约束的记录可以接收其中列出的普通记录类型。

未用于字段的类型参数也保留名义身份，例如 `struct Tag<T: numeric> { u32 id; };` 的 Tag<u32> 和 Tag<f32> 物理布局相同，类型仍不同。函数可从 Tag<T> 推导 T，不从字段反猜类型。float／uint 等内置别名在解析具体类型后规范为 f32／u32。显式实参、推导和有限集合交接同一实际类型身份，用于生成具体实例名。类型参数不成为物理数据中的标签。

编译器检查每份泛型记录的全部允许类型组合，包括未被 计算函数 使用的实例。类型实参个数、集合或名义类型推导不符报 B232；类型参数声明沿用 B231。记录的递归、空字段及普通字段重名继续报原诊断；dataset 字段形成拥有记录，按CPU应用资源规则使用；默认值、泛型记录的自动构造参数推导及开放接口约束尚未实现；u32记录const与形状推导见相应章节。

默认布局按具体字段的既有 size／align 规则计算，类型参数元数据不占 GPU／BLDATA 记录字节。Particle<float3> 的位置／速度／id 偏移为 0／16／28，步长为 32。相同名字、相同实参的名义类型保持一致；不同记录或不同类型实参不因布局相同就互换。类型名称和参数显示会进入布局说明，不代表物理 ABI 新增字段。

stride 放在记录名称及泛型参数之后，例如 `struct Transform<const S:u32> stride(S) {float3x4 matrix;};`。它指定总大小／元素步长，保留自然字段偏移与自然对齐；u32编译期表达式在具体化并代入记录类型／const参数后求值，例如`stride((N+1)*4)`；结果必须容纳自然布局且是自然对齐的倍数，否则报B241。填充不是普通值字段，类型化赋值不覆盖填充。完整规则、GPU 页寻址及原生所有权见 [记录步长与设备实例](DeviceInstances.md)。

现有 simulation.bodies 使用 BLBody<T: floating,const Power:u32 in {1,2,3}>，integrate_body 按实际 BLBody 推导类型与模型值并借用完整记录，measure_energy 按速度 T 推导；integrate／energy／step 计算函数 按 BLSimulatedBody 泛型，statistics 从实际 BLBody<float3,3> 自动推导。该布局的位置／速度／energy 偏移 0／16／28、步长 32 保持，源数据由原 Dataset 和并行域持有，没有增加一套模拟状态。

## 用户定义的有限类型集合

`typeset` 为泛型参数声明有限的具体类型清单。成员可以是普通内置值类型、普通记录或已指定实参的泛型记录；含不可变 view 字段的普通记录也可以列入。集合名称只用于类型参数约束，一个 dataset 仍绑定一种具体类型。

完整 `.h` 示例：

```c
module movers;
struct Point2 { float2 position; float2 velocity; u32 id; };
struct Point3 { float3 position; float3 velocity; u32 id; f32 weight; };
typeset Movable { Point2; Point3; };
void advance<P: Movable>(inout P point, f32 dt) on compute;
void step(inout dataset<Point3> points, f32 dt) on compute;
```

对应 `.b`：

```c
module movers;
void advance<P: Movable>(inout P point, f32 dt) on compute {
    point.position += point.velocity * dt;
}
void step(inout dataset<Point3> points, f32 dt) on compute {
    parallel (point in points) {
        advance(point, dt);
    }
}
```

advance 的两个成员实现分别按 float2、float3 字段检查，即使 计算函数 只调用 Point3，Point2 上的错误也会使库编译失败。类型从调用方实际参数推导，再展开到原字段、寄存器和访存操作；没有逐元素类型分派。集合也可约束返回值、write／inout 位置、dataset／view 元素、cooperative tile 和集合回调，资源权限、形状、索引、requires 和输出完整性仍由原规则检查。

类型集合按已有模块类型作用域解析，可放在 .h 或 .b，声明一次；使用其他模块的集合需按既有导入规则引用，也可写 `P: schema::Movable` 等限定名。.h/.b 的函数类型参数约束仍须一致。集合与记录共享类型名称空间，不能同名或覆盖内置类型及内置约束集合。

成员按完整名义类型解析；uint／u32、float／f32 等别名及重复成员在编译期去重，不合并不同名字或不同实参的同布局记录。集合可以列入 Particle<float3>，函数和记录可以用这些普通记录作为 T；类型替换保留完整嵌套实参及资源包装，不能只替换一个类型名字符串。

例如 `typeset Payload { Point2; Particle<float3>; };` 配合 `struct Envelope<T: Payload> { T value; u32 version; };`，可产生 Envelope<Point2> 与 Envelope<Particle<float3>> 两种具体布局。`T(...)` 按实际成员的原构造规则处理：普通数值继续显式转换／分量构造，记录按字段构造；该调用必须对全部成员合法。

集合至少包含一个完整具体类型；不能直接列出 dataset、tile、view 包装、未指定参数的泛型记录、集合名或 void。集合与记录约束不能递归引用。声明／解析／递归错误报 B233，成员类型错误沿用 B031／B228／B232，调用方类型不在集合中沿用 B231／B232。集合中的记录即便含有 view 字段，仍不能因此进入禁止 view 的 tile 或原子借用。

这是有限清单下的编译期多态。类型集合组合、开放编译期接口约束、关联类型及静态运算符重载尚未实现；不自动为成员补齐字段或选择其他算法。异构数据使用声明了全部载荷类型的enum／Result及数组、dataset组合，match按实际活动载荷执行，类型与分支在编译期检查；具体拥有元素存储范围仍见对应章节，不能由普通变体数组外推为动态拥有容器已完成。任意类型装箱后依赖运行期类型选择行为、类继承、虚方法分派及动态类型对象不属于B的设计方向，不列为待实现能力。实际引擎的BLSimulatedBody集合列出四种浮点维度与Power∈{1,2,3}的BLBody<T,Power>，integrate_body按每份完整BLBody的字段生成原计算。

## 泛型并行 计算函数 与阶段组合

计算函数 可以使用与普通／cooperative 函数、记录相同的有限类型约束。`计算函数 update<T: numeric>(inout dataset<Item<T>> items)` 或 `计算函数 step<B: BLSimulatedBody>(inout dataset<B> bodies, f32 dt, f32 strength)` 都是原生并行入口：元素类型在编译期确定，并行域、访问权限与阶段关系仍由 计算函数 声明。

完整 `.h` 示例：

```c
module generic_work;
struct Item<T: numeric> { T value; T bias; };
struct Total<T: numeric> { T value; };
void update<T: numeric>(inout dataset<Item<T>> items) on compute;
void sum<T: numeric>(read dataset<Item<T>> items, write dataset<Total<T>, 1> total) on compute;
void run<T: numeric>(inout dataset<Item<T>> items, write dataset<Total<T>, 1> total) on compute;
void entry(inout dataset<Item<uint3>> items, write dataset<Total<uint3>, 1> total) on compute;
```

对应 `.b`：

```c
module generic_work;
void update<T: numeric>(inout dataset<Item<T>> items) on compute {
    parallel (item in items) { item.value += item.bias; }
}
void sum<T: numeric>(read dataset<Item<T>> items, write dataset<Total<T>, 1> total) on compute {
    reduce_sum(items.value, total.value);
}
void run<T: numeric>(inout dataset<Item<T>> items, write dataset<Total<T>, 1> total) on compute {
    update(items);
    sum(items, total);
}
void entry(inout dataset<Item<uint3>> items, write dataset<Total<uint3>, 1> total) on compute {
    run(items, total);
}
```

entry 的 run、update、sum 都从实际 Item<uint3>／Total<uint3> 推导 T=uint3，不复制宿主逐元素循环。函数与 计算函数 使用同一套名义类型推导；多个实参必须给出相同类型，同布局的其他记录不能冒充 Item。可自动推导的类型来自实际参数，不能靠执行数据猜测；也支持显式 `run<uint3>(...)`，未出现在形参中的类型必须显式给出。

编译器检查每个泛型 计算函数 的全部允许类型组合，包含未使用／私有 计算函数 和条件模板；出错时附具体实例。普通并行写完整性、索引与数值证明、tile／cooperative 作用域、形状、设备目标、数据访问和参数别名保持原规则。泛型不授予整份数组的任意写权限，也不允许在 parallel 或普通值函数里调用 计算函数。

requires／ensures prefix 可以与泛型并用。调用先从实际绑定推导完整类型，再沿原 prefix 接口交付有效域；不同阶段保留同一 dataset 与 Count 的身份。生产者 compact 后可组合泛型消费与归约，GPU 中间计数留在设备；泛型不加入 Count 镜像、提交、读回、屏障或运行期类型分派。同域阶段仍按既有依赖融合，显式 phase 和块内协作边界仍有效。

入口模块公开的泛型 计算函数 按具体类型导出到原 Compilation.programs／templates，例如 `generic_work::run<uint3>`；导入模块的实例只供检查和调用，不变成当前入口模块的公开接口。现有 compute 命令接受完整名字或带完整类型后缀的本地名 `run<uint3>`；记录实参保留限定名，例如 `step<simulation.bodies::BLBody<float3,3>>`。未给类型的 `run` 不是可执行实例；有 requires 的模板仍只能经持有域关系的原生绑定或满足前提的 计算函数 组合执行。

bc build 的泛型导出文件使用 `模块_入口.instance-序号`，避免 Windows 文件名中的尖括号，并通过同名 .layout 记录完整的 计算函数 名字与布局；序号是本次导出列表的位置，不是稳定 ABI。普通具体入口的文件名不变。具体程序的原生 ABI 只包含实际资源与普通值参数，不含类型标签或泛型调度器。

simulation.bodies 的 integrate、energy、step 按 BLSimulatedBody 声明，覆盖四种浮点维度与三种 Power 的 BLBody<T,Power>。step 组合前两个泛型 计算函数；statistics 明确消费 BLBody<float3,3>，调用自动推导，两个 map 仍可融合成一个实际阶段。Power 只属于编译期模型身份，不增加物理字段；源码阶段数变化不等同于设备性能提高。

当前仍为有限集合的编译期多态，有限 u32 函数 const 参数也已支持；开放接口约束／关联类型、动态任务和通用符号泛型体分析未实现。笛卡尔积检查随类型参数组合增加，尚无按符号证明全部实例的分析；不把本节能力等同于完整语言。

## 有限编译期值参数与显式实例

普通函数、cooperative 函数与 计算函数 可以声明 `const N: u32 in {17,65,257}`，类型参数写在 const 参数之前。它定义有限的合法编译期值域；不指定具体入口的 check／build 与原类型集合一起逐实例检查，包括未被入口调用的有限实例。开放声明及具体入口选择规则见后文。不同形状、块宽度、固定循环上界或算法分支可以共用一份源程序。

完整 `.h` 示例：

```c
module specialized_blocks;
const u32 ChosenWidth = 65;
struct Cell<T: numeric> { T value; u32 audit; };
void add<T: numeric, const Bias: u32 in {1,7}>(inout T value) on compute;
void sum<T: numeric, const Width: u32 in {17,65}, const Bias: u32 in {1,7}>(inout dataset<Cell<T>> cells) on compute;
void entry(inout dataset<Cell<uint3>> cells) on compute;
```

对应 `.b`：

```c
module specialized_blocks;
void add<T: numeric, const Bias: u32 in {1,7}>(inout T value) on compute {
    value += T(Bias);
}
void sum<T: numeric, const Width: u32 in {17,65}, const Bias: u32 in {1,7}>(inout dataset<Cell<T>> cells) on compute {
    parallel (cell in cells) {
        let values = tile_input(cells.value, Width);
        T total = tile_reduce_sum(values);
        cell.value = total;
        add<T, Bias>(cell.value);
    }
}
void entry(inout dataset<Cell<uint3>> cells) on compute {
    sum<ChosenWidth, 7>(cells);
}
```

调用可以给出全部类型和 const 实参，例如 sum<uint3,65,7>，也可只给全部 const 后缀，例如 sum<65,7>，让类型从实际参数推导。不支持任意省略部分类型、const 默认值或从运行期 dataset 容量猜测 const；静态形状推导见开放 const 一节。不写尖括号时，可从形参对应的名义记录推导全部 const 参数，详见下节；不能推导的值仍须显式给出。普通值函数和 计算函数 都使用这一规则；协作调用写 `cooperate aggregate<T,7>(tile)` 或只给值后缀。

显式 `in {...}` 值域只接受非空的直接十进制 u32 字面量清单；省略 in 的开放声明见后文。有限清单重复值去重、顺序规范化；u32／uint 别名均可作其声明类型。调用const实参接受可完全编译期求值的u32表达式，包括模块const／限定名、外层const参数及派生算式，例如`next<N+1>()`或`next<(8>>1)>()`。类型实参顶层的`>`／`>>`属于尖括号闭合符；需要这些比较／右移运算时写在括号内，普通括号中的表达式保持原优先级和语义。有限`in {...}`值域仍是上述直接十进制清单，不扩展为区间、依赖条件或一般谓词。

const 参数作为固定 u32 值进入原类型、数值与索引分析，不保留字面量的隐式适配权限。它可用于本函数的 dataset<T,N> 形状、tile_input 的块宽度、普通数值、构造、分支／循环边界以及子程序的普通值参数；实例化时形状变成原 Buffer.count，值变成已有 Constant IR。局部变量、循环／parallel 绑定和普通参数不能覆盖本函数的 const 参数；不同函数自己的词法作用域仍独立。const 参数不能改写，也不能充当函数或普通类型。

显式类型实参同时支持原有限数值集合、用户类型集合和完整的泛型记录名。例如 copy<Pair<uint3>,7>(output,input)。仅返回类型中的参数可以通过 zero<uint3>() 等形式指定，不再强制它出现在形参中；未指定且无法推导时编译报 B231。记录构造 Pair<uint3>(...) 保持原名义类型与字段语义；记录的有限 u32 const 参数及混合实参见下节。

实例从原声明及本次类型／const绑定静态生成，完整检查与实际调用共用实例化入口，不保存全部具体定义的全局副本；阶段参数和阶段for直接读取原输入根及同一实例的u32 const值，保留名称来源，不要求替换源AST；运行期范围仍需放在parallel中，未知名称仍报原错误。不加入运行期类型／值分派、额外 uniform、设备参数表或初始化阶段。实际调用仍静态内联，计算函数 保持原阶段、融合、phase、访问、前缀有效域和完成所有权。静态块宽度不等于物理 subgroup 宽度；cooperative 函数仍继承真实 tile 作用域，不因增加 const 参数就能另建隐藏固定块。循环边界已知不表示循环一定展开，编译期值也不能替代设备真实有效 Count 或伪造 prefix 关系。

入口模块的公开 计算函数 为每个类型／值组合导出具体实例，例如 `blocks::run<uint3,65,7>`、`sized::lookup<17>`；compute 命令按这一完整后缀选择。导入／私有实例仍只服务已有调用与检查，bc 使用原 instance 序号文件名。类型与值组合的检查量随笛卡尔积增长，不承诺无限参数集合或零编译成本。

声明、值域、实参数量／成员和非法遮蔽报 B235，类型实参与推导继续使用 B231／B232／B124；签名不一致 B028、形状 B167、权限／别名、初始化、整数除数和协作的诊断保持。含 {0,17} 的块宽度程序会因 0 实例不合法而拒绝，不能只挑 17 隐藏它；递归仍拒绝。函数写效果在显式调用中同样进入原可变性、确定初始化与当前内容证明。

实际 math.field::central_acceleration 以 Power∈{1,2,3} 选择软化半径的分母 r、r²、r³，r²=dot(position,position)+0.25；这表示分母幂次，不把三者的物理力幅衰减混称为同一参数。simulation.bodies 的 BLBody<T,Power> 持有编译期模型身份，integrate_body 从实际记录推导 T／Power，再调用 central_acceleration<Power>。statistics 选择 BLBody<float3,3>，保留原三维算法、物理布局和机器码；其他模型由各自实例求值，不增加运行期模式分支。

开放u32 const、固定内联数组及静态形状推导见对应章节。一般谓词值域、i32／bool／浮点／记录值参数、通用模板递归、符号泛型体分析及一般编译期循环解释仍待实现。

## 记录 const 参数与名义值推导

记录也可声明有限 u32 参数，例如 `struct Cell<T:numeric,const Width:u32 in {17,65},const Bias:u32 in {1,7}>`。类型参数在前、const 参数在后，声明与函数共用值域语法。类型和构造必须完整给出实参，例如 `Cell<uint3,65,7>`、`Cell<uint3,ChosenWidth,7>(value,audit)`；记录自身不做构造参数推导，也没有默认值。

完整 `.h` 示例：

```c
module typed_blocks;
const u32 ChosenWidth = 65;
struct Cell<T: numeric, const Width: u32 in {17,65}, const Bias: u32 in {1,7}> { T value; u32 audit; };
void add<T: numeric, const Width: u32 in {17,65}, const Bias: u32 in {1,7}>(inout Cell<T,Width,Bias> cell) on compute;
cooperative T total<T: numeric, const Width: u32 in {17,65}, const Bias: u32 in {1,7}>(tile<Cell<T,Width,Bias>> input) on compute;
void sum<T: numeric, const Width: u32 in {17,65}, const Bias: u32 in {1,7}>(inout dataset<Cell<T,Width,Bias>> cells) on compute;
void entry(inout dataset<Cell<uint3,ChosenWidth,7>> cells) on compute;
```

对应 `.b`：

```c
module typed_blocks;
void add<T: numeric, const Width: u32 in {17,65}, const Bias: u32 in {1,7}>(inout Cell<T,Width,Bias> cell) on compute {
    cell.value += T(Bias);
}
cooperative T total<T: numeric, const Width: u32 in {17,65}, const Bias: u32 in {1,7}>(tile<Cell<T,Width,Bias>> input) on compute {
    T sum = tile_reduce_sum(input.value);
    return sum;
}
void sum<T: numeric, const Width: u32 in {17,65}, const Bias: u32 in {1,7}>(inout dataset<Cell<T,Width,Bias>> cells) on compute {
    parallel (cell in cells) {
        let input = tile_input(cells, Width);
        T sum = cooperate total(input);
        cell.value = sum;
        add(cell);
    }
}
void entry(inout dataset<Cell<uint3,ChosenWidth,7>> cells) on compute {
    sum(cells);
}
```

entry、sum、total 与 add 从真实的记录类型取得 T=uint3、Width=65、Bias=7。名义值推导也可递归穿过嵌套记录、dataset／tile 元素及不可变 view 元素；同一参数出现多次时必须一致，推导结果必须属于被调函数值域。只省略全部泛型实参时自动推导全部参数；显式调用仍须给完整清单或全部 const 后缀。不从预期返回类型、运行期字段值或资源容量猜测值，`dataset<T,N>` 的固定容量单独出现不足以推导 N。

const 值属于名义类型身份。`Cell<uint3,17,7>` 与 `Cell<uint3,65,7>` 是不同类型，即使字段布局相同也不能互传。Ty 保存实际类型和值参数供编译器恢复完整类型语法，值不占用记录载荷、资源描述或普通 uniform；实际对象只有声明的字段。记录字段可以把外层值转交给内层记录，用户类型集合可列出具体值实例。模块 const 先求值再规范化身份，`ChosenWidth` 和字面量 65 指同一实例；参数名不是运行期成员或静态成员访问语法。

记录 const 参数本身不是内联存储或通用依赖类型；固定数组见下节。Width 身份本身不证明任意 view 长度、不冻结有效 Count、不授予索引／写权限；上例的块宽度来自函数绑定的编译期 Width，尾块继续使用真实有效长度。全部合法记录和函数组合仍在编译期间检查，未被入口使用的非法嵌套值实例也会拒绝。

原声明是唯一函数定义表，具体实例由检查或调用在自身生命周期内生成；类型解析中的命名 const 即使调用泛型函数，也无需等待另一张实例表填充。常量与类型互相依赖时，共用本次编译的活动常量集合，直接报 B160，诊断返回后移除该活动项；没有运行期初始化、值缓存或重试路径。未知／非 u32 命名值沿原 B015 入口拒绝，记录数量／类型位置错误 B232、值域与不可推导／冲突 B235、普通类型不匹配 B124。

## 固定维度矩阵与空间变换

原生 f32 矩阵为 float2x2、float2x3、float2x4、float3x2、float3x3、float3x4、float4x2、float4x3、float4x4。floatRxC 的 R 为行数，C 为列数。它们是独立普通值类型，可用于字段、局部、模块 const、函数参数／返回／独占借用、dataset／tile／不可变 view 元素、用户类型集合和集合算法，不属于原 numeric／floating 数值集合。

矩阵按行布局，每行遵守 floatC 的对齐：两列行大小／对齐／步长为 8 字节，三列行为大小 12、对齐与步长 16，四列行为大小／对齐／步长 16。矩阵大小是 R×行步长，对齐等于行对齐；float3x3 大小 48 字节、float4x3 大小 64 字节。逻辑分量不包含行内填充，运算和普通写入保留目标填充；矩阵没有运行期维度字段、对象分配或另一份存储。

构造的分量顺序为行优先，可给完整标量清单、拼接标量／向量或每个行向量。例如 float2x3(1.0,2.0,3.0,4.0,5.0,6.0) 与 float2x3(float3(1.0,2.0,3.0),float3(4.0,5.0,6.0)) 相同；构造显式把普通数值分量转换成 f32。单个标量构造对角值，其余分量为 +0.0：float3x3(1.0) 是单位矩阵，float2x3(1.0) 的前两条对角线为 1；它不是全分量填充。相同矩阵类型可直接复制构造，不接受不同矩阵或普通记录／资源的构造替换。

行字段为 r0 至 r(R−1)，类型为 floatC，可继续 swizzle、赋值或按原规则独占借用。例如 m.r1.xy 保持该矩阵的真实位置；行内／整矩阵的借用冲突及确定初始化仍按原逻辑分量检查。矩阵不能通过 float4(m) 等数值构造丢失维度，先取得所需行字段。

| 表达式 | 语义 |
|---|---|
| A + B、A − B | 维度相同的矩阵逐分量加减 |
| −A | 逐分量取负 |
| A × B | 线性代数乘法，A 的列数等于 B 的行数，结果维度为 A 行数×B 列数 |
| A × v | v 为列向量，维度等于 A 列数；结果为 A 行数的向量 |
| v × A | v 为行向量，维度等于 A 行数；结果为 A 列数的向量 |
| A × s、s × A、A / s | s 为 f32 标量，逐分量缩放／除法；数值字面量沿原显式适配规则 |
| transpose(A) | 行列互换，结果类型为 floatCxR |
| determinant(A) | 2／3／4 阶方阵的 f32 行列式 |
| inverse(A) | 2／3／4 阶方阵的逆矩阵 |

表中 × 在源码中写 *。矩阵乘法的每个内积按 k=0、1、… 依次乘加，不强制 FMA 或改变括号；矩阵之间的 * 不是逐分量乘法。加减不广播标量，标量除以矩阵、矩阵比较／位运算和把 dot／normalize 等向量内建直接用于矩阵均不支持。维度、运算及构造种类错误报 B237，构造分量数 B157、返回／赋值类型 B124、行／分量 B141／B140，原权限、别名和初始化规则保持。

矩阵在前端静态展开为原标量值 IR，按已有优化、物理寄存器分配及 x64／SPIR-V 后端执行。复制／构造／转置重用分量，乘法生成对应的标量乘加；没有宿主逐元素回调或额外 GPU 数学库。行列式与逆矩阵按固定阶数展开余子式；逆矩阵是伴随矩阵除以行列式，不加奇异检查、单位矩阵替换或阈值修正。零行列式可产生 Inf／NaN，病态矩阵可能放大误差；这些数值性质不能由类型／维度正确推导。

矩阵构造、相同类型复制与 transpose 可以形成完全求值的模块 const；例如 const float3x3 Unit=identity<float3x3>()。当前一般 f32 二元运算、行列式／逆矩阵的编译期求值仍未实现，不能将它们的执行运算当作 const 初始化。

math.transform 的 BLMatrix 集合列出上述九个类型，BLSquareMatrix 集合列出三个方阵；identity<T>() 生成对角 1，compose<T>(first,second) 返回 second*first，scaled<T> 缩放矩阵。库还提供 transform_point(float4x4,float3)，包含齐次 w 除法；transform_plane(float3x2,float2) 映射平面偏移；transform_direction(float3x3,float3) 映射方向；transform_normal(float3x3,float3) 使用逆转置后归一化。

完整 `.h` 示例：

```c
module matrix_work;
import math.transform;
struct Pose { float4x4 local_to_world; float3 normal; float3 position; u32 audit; };
struct Sample { float3 position; float3 normal; u32 audit; };
void run(read dataset<Pose> poses, inout dataset<Sample> samples) on compute;
```

对应 `.b`：

```c
module matrix_work;
void run(read dataset<Pose> poses, inout dataset<Sample> samples) on compute {
    parallel (sample in samples) {
        let i = index();
        if (i < poses.length) {
            Pose pose = poses[i];
            float3x3 basis = float3x3(pose.local_to_world.r0.xyz,
                pose.local_to_world.r1.xyz, pose.local_to_world.r2.xyz);
            sample.position = transform_point(pose.local_to_world, pose.position);
            sample.normal = transform_normal(basis, pose.normal);
        }
    }
}
```

实际 render.trace 使用 float3x2 相机平面基乘以采样坐标，透视方向和正交原点共享该偏移；原相机参数与平台资源布局保持。矩阵也可通过 identity<T>／compose<T> 静态引用参与有序归约和扫描，原有效前缀与数据所有权不变。

当前是九种固定维度 f32 矩阵，不是一般维度依赖类型、稀疏／超大矩阵、动态矩阵行下标或完整线性代数系统。矩阵可作为下一节固定数组的元素；动态数据与大规模集合继续使用既有 dataset／view／并行集合，完整语言仍需继续。

零分量普通记录保留原名义身份、字段和自然对齐。无字段记录大小0、对齐1；只有零尺寸字段的记录大小0并保留字段所需对齐。它们各有唯一普通值，可构造、按值交接及嵌入记录／数组；没有数值分量便不属于数值类型。void仍由原数据类型规则B228拒绝，不以空记录替代void。零分量普通值的B位置借用没有机器引用，仍按原源类型和访问关系检查；普通C ABI不直接传递零尺寸值，含零尺寸字段的非空记录沿实际物理字段布局交接。

## 固定内联数组与原生下标
`array<T,N>` 是按元素布局存放 N 个 T 的固定内联聚合；实际元素不拥有资源时为普通值；N=0没有元素，亦为普通值。CPU 应用还支持下述固定拥有数组，N为包含0的u32编译期值，可由字面量、模块const、具体const参数或使用它们的编译期表达式取得；它与dataset的零长度形状范围分别按实际类型规则处理。类型身份包含完整元素类型和长度，`array<u32,2>` 与 `array<u32,3>` 不能互传；泛型函数能从数组元素和固定长度推导类型／const 参数，仍须满足声明的有限集合。不从 dataset 容量或运行期字段推测 N。

普通值元素可为标量、向量、矩阵、记录、另一层固定数组或不可变 view；tile 借用和 void 不能嵌入。普通值数组可作为字段、函数参数与返回值、write／inout 位置和 dataset／tile／view 元素。拥有 dataset、拥有记录／Result 的数组按“固定数组拥有资源与动态借用”组织，不作为 dataset／tile／view 元素；整份拥有位置借用见后文“拥有聚合的write与inout”。含 view 的数组沿原引用所有权传递，不能作为 计算函数 写出数据或普通 uniform；主机仍通过 with_views 绑定实际子资源。

布局为元素对齐、元素步长 align_up(T.size,T.align)、总字节数 N×步长。array<float3,3> 大小 48、对齐 16，逻辑分量不含每个 float3 的填充；数组普通赋值保持填充。array<array<u32,2>,3> 大小 24。长度不增加载荷或运行期状态，`.length` 返回固定 u32 常量。布局超出 u32 字节范围在类型解析时拒绝；这不代表现有私有栈、GPU 共享内存或单条分页记录能够容纳任意接近该上限的值。

`array<T,N>(v0,...,vN_1)` 按序构造全部元素。普通值数组的一个 T 参数填充 N 份，该参数只求值一次；一个相同普通值数组类型参数复制该数组。构造只为真实存在的结果分量形成值与借用事实，不先复制 N 份完整中间值；零分量结果仍保留原类型和求值效果。拥有数组必须逐项交付资源，不能复制单个拥有元素填满；一个相同拥有数组类型参数移动整份数组。元素需要准确的 T，不将异构值或整个数组隐式转换成数值向量。普通值构造、相同类型复制与常量下标可形成模块 const；运行期控制流和一般编译期循环／浮点求值仍未补齐。

N=0时，`array<T,0>()` 是保留完整元素类型身份的唯一空值，大小0，对齐仍为T的对齐；没有元素、资源句柄或借用来源。普通填充值即使N=0也先求值一次并按T检查，不通过丢弃填充值省略类型检查或观察效果。空数组的复制、返回、泛型实例与read／write／inout位置沿原值规则交接，写完整空位置无需制造一次写入。没有合法的空数组下标；以空域index为前提的函数没有可取输入，原条件流不生成该路径的实际访问。

数组以 `a[i]` 选取元素，可继续字段、向量 swizzle 或下一层数组下标。i 必须是 u32，并已由常量、区间／条件、有界 for 或 `requires index(i,values)` 证明小于 N；长度相同的固定数组共享这一静态范围事实。没有逐元素运行期越界分支。无法证明的下标报 B137；数组下标仍先求值并完成原类型及有效性检查；选中元素没有逻辑分量时，直接按原类型、基底和访问权限交接空位置投影，不生成没有载荷消费者的索引偏移、地址、装载或写入。下标的副作用、越界诊断和真实资源生命周期保持。动态临时数组索引已完成原下标有效性检查后，在形成实际存储时只捕获一次输入事实，后续选择直接消费同一份事实。向量按其实际分量交接，不经过数组物化。选择只为实际结果分量形成所选Proof。零分量元素直接交回空Proof并保留原路径，不构造没有结果消费者的逐元素候选及条件并图；数组与下标的真实求值、副作用、类型、边界、权限和资源寿命保持。数组不提供 .x 等向量分量，也不参与数值算术、比较、向量／矩阵的展平构造或 subgroup 首值广播，先选取元素。

下述动态初始化、整份位置借用及设备私有数组适用于普通值元素；拥有资源元素遵循前述固定拥有数组规则。普通值的常量下标按原逻辑分量投影；动态下标按其已捕获的实际值身份选取原位置，可直接写入未赋初值的局部数组、write 数组形参、CPU dataset 记录或 计算函数 独占元素。写后可读取同一选中位置，改变下标不会继承旧位置的初始化。固定起止范围的有界 for 若在每条实际继续边写入当前元素，则在自然出口交付相应位置；嵌套数组、字段、向量分量、分段范围和完整 write 输出共用这一关系。continue、break、return 和 seek 保留各自实际路径，漏写不能因其他路径写全而通过。动态写仍撤回候选存储的旧数值事实，不把新值的范围赋给全部候选元素。动态元素可交给 write／inout；同次调用中可能相交的动态位置按冲突处理，即使两个下标来自不同寄存器。write 参数用本次形参的编译期初始化身份检查，调用方原有值不会令非法的先读后写通过；这些身份没有执行期载荷。

一般 while 归纳、依赖运行期端点的任意覆盖、动态前缀与容量尾部的初始化组合仍待实现；合法但尚不能证明的源码须作为能力缺口记录，不能改写成非法程序。

CPU 的动态私有访问使用原生栈上的连续字，GPU 使用 SPIR-V Function 数组；静态分量继续参与原寄存器和死代码优化。全局／view 动态访问直接在原记录地址上加偏移，不加载整条记录后逐项选择。tile 输入／发布动态读取保持共享流的物理字顺序和真实生命周期，跨发布点仍保存当前 lane 的私有活跃值；尾块只读取有效 lane。没有宿主逐元素回调、第三方数组运行时或另一个任务调度器。

主机 `Ty.field`／Dataset.field_offset 和 with_views 路径支持 `normal_indices[2]`、`refs[1]`、`groups[0].child` 及根数组 `[0]`，按同一类型布局求偏移。输入携带的引用必须有实际 ReadDataset 所有者，原始头部字节不能替代绑定。表达式中的下标和字段基对象只求值一次，包括读取数组内 view 的 length。

完整 `.h` 示例：

```c
module fixed_arrays;
const u32 Width = 4;
struct Cell { array<float3,Width> offsets; u32 selected; };
void shift(inout dataset<Cell> cells, float3 delta) on compute;
```

对应 `.b`：

```c
module fixed_arrays;
void shift(inout dataset<Cell> cells, float3 delta) on compute {
    parallel (cell in cells) {
        u32 selected = cell.selected;
        if (selected < cell.offsets.length) {
            cell.offsets[selected] += delta;
        }
    }
}
```

泛型签名可写 `T at<T:numeric,const N:u32 in {2,3,4}>(array<T,N> values,u32 i) on compute requires index(i,values);`，函数体直接 `return values[i];`。这里的约束证明来自调用实参，不增加运行期参数或检查。

对固定数组的 requires index(i,values)，函数体同时取得 i 的数值范围 0 <= i < N；向量索引的每个选中分量分别取得此事实。已有算术与路径分析可继续使用它，例如 N=17 时，values[i >> shift]、长度为 18 的另一数组的 [i+1] 和以 17-i 为除数均可证明；无须重复写同一长度条件。调用点仍须先证明原契约。赋值替换索引值后不沿用旧事实，可能回绕的 i-1 或仍可能为零的 i 不能因此直接用作下标或除数；动态资源的长度与并行独占写权限分别证明。

现阶段固定数组用于普通值聚合，编译器仍按逻辑分量展开类型与整体值，动态访问使用同一根的连续存储。GPU 私有数组按声明布局确定存储单元，B 发射同一根的固定与动态地址。固定位置直接采用实际单元编号，8 位单元的动态地址直接使用字节下标。编译成本会随内联长度增长，不能据此宣称适合超大容量私有数组。大规模数据继续由 dataset／view 分页和并行集合承载。固定数组的动态 u32／i32 原子下标已支持，沿显式 atomic dataset／tile 权限与内存序执行，见下节；B214 拒绝非原子目标或普通读写混用。动态长度内联数组、切片、矩阵动态行下标、一般依赖类型与完整语言继续开发。

实际 render.trace 的 Triangle 已用 array<u32,3> normal_indices，宿主按同一数组路径写入三项，三角形仍为 12 字节；着色的三个常量下标消除为原直接访问，不改变原高精度几何或采样。

## 显式原子 dataset 与内存序

`.h` 用 `atomic dataset<T>` 声明共享原子访问能力，`.b` 必须保持相同签名。它要求已初始化的可读写数据，布局仍是原来的普通记录；支持u32／i32／u64／i64标量位置的原子操作，可选择嵌套字段、向量的一个分量或已证明下标的固定数组元素。其他字段和填充不会被隐式读改写。它不是裸指针或可逃逸的原子引用；普通函数可以显式借用整份 atomic dataset，计算函数 继续拥有并行域与阶段组合。

```c
struct Sample { u32 key; u32 value; };
struct Bin { u32 count; u32 sum; };
void histogram(read dataset<Sample> input,
                  atomic dataset<Bin> bins) on compute {
    parallel (item in input) {
        let n = bins.length;
        if (n != 0) {
            let k = item.key % n;
            atomic_add(bins[k].count, 1, relaxed);
            atomic_add(bins[k].sum, item.value, relaxed);
        }
    }
}
```

长度仍是只读元数据，索引仍须由同一前端证明。多个 invocation 可以原子修改同一字段；该模式下，直接 `bins[k].count` 读写、把整条记录作为普通值读取，或用 tile_input 捕获其普通快照都被拒绝。read/write/inout 位置、局部变量和不可变 view 不能当作原子目标。原子目标错误报 B214，目标不是支持的整数标量或请求未实现的64位tile原子报B215，参数数量报 B217，值类型沿用 B124。

| 内置操作 | 返回值 |
|---|---|
| `atomic_load(target, order)` | 当前原子值 |
| `atomic_store(target, value, order);` | 语句，无返回值；绑定或用于表达式报 B218 |
| `atomic_add`／`atomic_sub`／`atomic_exchange(target, value, order)` | 修改之前的旧值 |
| `atomic_min`／`atomic_max(target, value, order)` | 旧值；i32／i64使用有符号极值，u32／u64使用无符号极值 |
| `atomic_and`／`atomic_or`／`atomic_xor(target, value, order)` | 旧的完整32／64位值 |
| `atomic_compare_exchange(target, expected, desired, success_order, failure_order)` | 旧值；与 expected 相等才写 desired，是不发生伪失败的强比较交换 |

所有操作都显式提供静态内存序：`relaxed`、`acquire`、`release`、`acq_rel`。这些是该参数位置的语法，不接受运行时变量。load 只接受 relaxed/acquire，store 只接受 relaxed/release，读改写接受四种序。比较交换失败序只能 relaxed/acquire，且不能强于成功序；非法序报 B216。值参数不获得额外隐式数值转换，加减按目标实际宽度保留低32／64位。此语言的 CPU／GPU 共同配置不支持 seq_cst，也不把它替换成较弱的序；依据为 [Vulkan 内存模型](https://docs.vulkan.org/spec/latest/appendices/memorymodel.html)。

GPU 使用 Device 作用域，约束当前设备上的 buffer 原子；CPU 约束访问同一主机数据的执行线程。它们不会把主机和显存变成同一个地址或同一个原子域，显式上传／下载与资源所有权仍由原框架负责。relaxed 只约束该位置的原子性；release/acquire 只有实际读到对应发布时才建立同步，不是全域屏障，也不保证跨 workgroup 自旋循环能够推进。全域阶段继续由 计算函数 依赖和 phase 表达。

同一数据集传给多个可写子程序参数时，只有这些别名全部声明 atomic 才能共享；混合普通读写别名仍报 B118。拥有 inout/atomic 能力的外层 计算函数 可调用相应子程序，read 或 write 单独不能升级成需要读写的 atomic 能力。不同访问阶段之间，编译器按真实读、写和跨元素访问保留依赖；原 cross_reads 统一为 cross_access，涵盖原子写入，不能把“先读取整域、再跨元素原子写入”错融成逐元素执行。

原子效果不能作为纯表达式消除、缓存或提前到未选分支，忽略旧值也必须执行修改。CPU 自研 x64 直接使用对齐 MOV、LOCK XADD、内存 XCHG、LOCK CMPXCHG；极值和按位读改写由 CMPXCHG 构成同一个原子操作。GPU的B发射器按原操作、位宽和有符号性写出SPIR-V原子指令，交接实际指针、作用域和内存语义ID。宽整数使用同一物理页地址的64位指针及Int64Atomics，64位store复用OpAtomicExchange并丢弃旧值以落实原子写入，原relaxed／release源合同保持；原Device按程序查询、启用[shaderBufferInt64Atomics](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceShaderAtomicInt64Features.html)，未声明或不支持时明确拒绝。acquire／release使用UniformMemory语义；不增加宿主锁、逐元素回调或另一种 Shader 编译器。指令和比较交换的成功／失败规则见 [SPIR-V 原子规范](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html#OpAtomicCompareExchange)。当前 CPU 原子阶段使用原生标量范围并行，尚未实现原子操作的四元素 SIMD。

前缀域在 CPU 启动范围／GPU 间接参数中已经捕获本次有效数量。阶段内原子改写原 Count 不改变本次范围；既有 written 规则撤销下一阶段的旧域关系，不能继续使用旧 prefix 或声称保留其 ensures。原子增量、返回的票号或单独的 Count 都不能自行签发安全散写／容量／prefix 能力。返回旧值依赖执行次序，CPU 与 GPU 的逐行票号可以不同；可交换整数归约的最终结果与票号唯一性分别评价。

块内u32／i32共享原子与显式根块屏障见下文。64位tile原子、浮点原子、可变view、动态任务及第一类有界预留类型仍未实现；dataset的u64／i64原子按本节实际设备能力执行，不代表已有完整并发队列。

### 原子数组下标与函数借用
`atomic_add(bank.rows[row][slot].count,1,relaxed)` 可直接选择嵌套固定数组里的u32／i32／u64／i64 dataset位置，每层下标由原范围分析证明。类型、记录对齐、原子访问权和内存序保持，不转成逐项选择，也不增加宿主锁、回调或逐元素越界检查。

原 Atomic IR 增加可选动态字偏移，来源仍是已检查的 Place；地址依赖进入原寄存器活跃性、重映射和阶段效果。CPU／GPU 在原记录／共享存储地址上加偏移，再发出原子操作；固定下标保持原直接位置。Device 分页先按记录索引选择页面，数组偏移留在该记录内；Workgroup 索引保留共享流顺序、发布值与原生命周期。

普通函数可借用整行 `atomic array<T,N>` 或动态选出的记录／标量位置，N 从实际数组类型推导。`requires index(slot,bins)` 适用于原子或独占借用的固定数组及记录数组字段，长度证明只读取类型元数据；数组 length 仍是静态值。原子结果只给出旧整数，不授予任意散写或输出容量权限。

`.h` 示例：

```c
module histogram;
const u32 BinCount = 256;
struct Sample { u32 key; };
struct BinBlock { array<u32,BinCount> counts; };
u32 increment<const N:u32 in {64,256}>(atomic array<u32,N> bins,u32 slot) on compute requires index(slot,bins);
void count(read dataset<Sample> input,atomic dataset<BinBlock,1> bins) on compute;
```

对应 `.b`：

```c
module histogram;
u32 increment<const N:u32 in {64,256}>(atomic array<u32,N> bins,u32 slot) on compute requires index(slot,bins) {
    return atomic_add(bins[slot],1,relaxed);
}
void count(read dataset<Sample> input,atomic dataset<BinBlock,1> bins) on compute {
    parallel (sample in input) {
        increment(bins[0].counts,sample.key%BinCount);
    }
}
```

主机提供已初始化的 BinBlock；此程序在原计数上累加，空输入保持计数。具有相同权限的 tile_atomic 数组也可复用 increment；在全块更新后读取最终值，仍由协作根块使用既有 tile_barrier 明确同步。原子操作不等于全块完成，release／acquire 不隐式产生根块屏障。

非原子局部、普通read／inout、不可变view及只读tile不能作原子目标，atomic数组元素不能混用普通读写。目标宽度、权限、下标／前提和内存序由原B215／B214／B137／B226／B216诊断；dataset原子支持本节列出的32／64位整数，tile仍仅32位。动态长度内联数组、浮点／64位tile原子、第一类原子引用及动态任务仍未实现。

## 普通函数的原子字段借用

普通函数可以用 `atomic T parameter` 借用一个实际原子位置，不把它读成 T 的普通值。T 可以是标量、向量、固定数组或不含资源 view 的非空记录；实际原子操作接受 u32/i32/u64/i64 标量字段；tile 的原子标量和实际设备要求见“块内原子共享值与显式屏障”。`.h` 与 `.b` 的 atomic 访问模式必须一致，它明确声明共享访问效果，不能当作纯数值参数。接口示例：

```c
module counters;
struct Count { u32 value; };
struct Job { u32 old; };
u32 add(atomic u32 target, u32 delta) on compute;
void update(inout dataset<Job> jobs, atomic dataset<Count> counts) on compute;
```

对应实现：

```c
module counters;
u32 add(atomic u32 target, u32 delta) on compute {
    return atomic_add(target, delta, relaxed);
}
void update(inout dataset<Job> jobs, atomic dataset<Count> counts) on compute {
    parallel (job in jobs) {
        let n = counts.length;
        if (n != 0) {
            job.old = add(counts[ordinal(job) % n].value, 1);
        }
    }
}
```

调用方必须已经拥有 atomic dataset 或 tile_atomic 的实际元素／字段，或者已有原子参数；局部变量、普通 read/write/inout 数据、只读 view 和只读 tile 不能升级为原子位置。索引先按原有边界规则证明，再借用实际位置。索引与普通值实参按从左到右求值一次，之后多层转交保留该位置；其中有副作用的索引计算不能被重新求值或删除。

函数内部可以显式 atomic_load/store、执行其他原子算子，或者把字段继续借给另一函数。整记录与向量借用保留实际逐 word 映射，支持嵌套字段、swizzle 和多个显式原子别名；同一位置可传给两个 atomic 参数，访问仍必须是原子操作。直接读取、赋值、复合赋值、普通局部复制、记录构造或返回借用均报错。借用不是普通 T 值、整数地址、可存入资产的数据或可逃逸引用；不提供地址算术，也不产生新的数据所有权。

cooperative 函数也可同时借用 tile<T>／atomic tile<T> 与 atomic T 字段。普通函数中的原子访问不隐式添加屏障，不允许隐藏 cooperate、tile_publish 或 tile_barrier；同步步骤仍由协作根块明确表达。调用目标继续遵守 cpu／compute／gpu 能力；函数抽取不会把 Workgroup 地址升级成 Device 地址，也不会把普通局部变量变为共享存储。

元素／cooperative函数的独立检查持有符号位置，封闭计算函数时按调用者的真实Place静态内联；普通CPU应用原生函数则按既有位置参数签名绑定真实CPU地址，并保留atomic权限。后端统一消费IndexedSource的Data／Tile／View原子位置，CPU View只表示已有CPU存储，不向源语言暴露可逃逸指针或新增原子资源ABI；不增加宿主锁或逐元素回调。实际读写进入原来的阶段依赖、跨元素访问、值转发与活跃性；返回旧值未使用时，写效果照常保留。经函数借用修改 Count，同样撤销后续旧 prefix／ensures，当前执行仍使用已捕获的范围。

仅普通值参数的归约／扫描回调继续要求无共享写副作用，atomic 参数不能进入其接口。含 view 的原子参数报 B223；普通读写／非法实际来源沿用 B214，字段类型 B124，原子目标类型 B215，内存序 B216，签名 B028，目标能力 B161，递归 B160。未知位置索引不会因函数接口而获得证明。

atomic T 借用一个已有位置；整份只读／原子 dataset 借用见下一节。独占可写数组借用、第一类原子引用变量、函数返回原子引用、资源泛型和动态调用仍需按真实所有权与并行语义继续实现。

## 整份数据集的函数借用

普通函数与 cooperative 函数可显式接收 `read dataset<T>`／`atomic dataset<T>`，也可声明固定形状 `dataset<T,N>`。它们直接借用调用方已有资源，不复制 payload，不增加设备地址、运行时引用对象或宿主任务。函数库可以独立检查；计算函数 从实际调用点静态内联，CPU x64 与 GPU SPIR-V 沿用同一 Load／Atomic IR。

`.h` 示例：

```c
module resource_functions;
struct Cell { u32 value; };
struct Job { u32 key; u32 result; };
u32 lookup(read dataset<Cell> table, u32 key) on compute;
u32 visit(atomic dataset<Cell> bins, u32 key, u32 value) on compute;
void run(read dataset<Cell> table, inout dataset<Job> jobs,
            atomic dataset<Cell> bins) on compute;
```

对应 `.b`：

```c
module resource_functions;
u32 lookup(read dataset<Cell> table, u32 key) on compute {
    u32 result = 0;
    if (table.length != 0) { result = table[key % table.length].value; }
    return result;
}
u32 visit(atomic dataset<Cell> bins, u32 key, u32 value) on compute {
    u32 old = 0;
    if (bins.length != 0) { old = atomic_add(bins[key % bins.length].value, value, relaxed); }
    return old;
}
void run(read dataset<Cell> table, inout dataset<Job> jobs,
            atomic dataset<Cell> bins) on compute {
    parallel (job in jobs) {
        job.result = visit(bins, job.key, lookup(table, job.key));
    }
}
```

参数按实际资源身份绑定：交换参数名、多层转交或让两个同权限参数指向同一数组，都保留同一缓冲、字段布局和长度证明。只读别名允许，显式原子别名也允许；原子返回的获胜次序可以不同。CPU／GPU元素阶段的实参可以是已有数据集名称或其嵌套记录字段路径，例如samples.diffuse；字段借用保留原binding、真实字节偏移、记录步幅、容量和长度身份，不物化字段数组。转借、普通下标、只读游标、unsafe_read、泛型与既有形状／索引合同消费同一投影；不能传入单个元素、prefix表达式或普通值冒充dataset。普通CPU应用及独立模块的read／write／inout／atomic dataset参数同样支持字段投影：原句柄、长度、记录步幅与字段偏移随原生签名交接，多层转借和再次投影保持真实布局；同一具体类型／const实例可读取不同父记录中的同型字段。原初始化、形状、长度条件、访问权限及返回view来源合同消费同一真实资源。不同字段可分别独占或共享借用，重叠的可写借用仍拒绝；write及ensures initialized只交付所选字段，不能据此认定父记录其他字段已初始化，其他字段的初始化和编译期内容事实保持。普通CPU可写载荷地址按实际行、字段及动态下标合成的字节位置和标量宽度撤回重叠计数来源，未写兄弟字段和不同行的运行期prefix关系保持。不可变view携带真实元素步长，字段转换保留原存储布局；CPU／GPU整域入口可消费同一read／write／inout／atomic字段投影，原输出保证按实际字段偏移初始化所选位置；同根不同字段允许非重叠read／write／inout／atomic借用；写入只撤回实际可能来源中被写字段的内容事实，兄弟位置及已复制值的独立关系保持；atomic要求所选字段已初始化，沿原原子权限、别名与真实指令执行；普通CPU应用atomic dataset原生参数及atomic T转借复用真实CPU地址与原子操作，GPU整域字段投影沿原物理页数组、步幅和字段偏移交接，不物化字段数组或增加资源拥有表。元素／cooperative阶段的数据集借用不能作为普通值复制、存进设备记录或从函数返回；read 数组中的不可变 view 可以按原有值语义读出并返回，资源寿命仍由真实根与子所有者持有。


CPU整域read／write／inout／atomic调用的逻辑字段保留原句柄、实际字段偏移及初始化关系；Frame仍拥有原父记录、记录数、步长与存储位置。跨模块组合交接同一引用，不按父布局复制普通函数或生成列副本。整域原生参数只携带根句柄／offset，运行库从原存储取得count／stride；普通CPU函数的四分量借用签名继续服务其实际载荷访问。

集合内建入口的资源字段／prefix实参由域展开解释，on cpu不会仅因命名字段表达式被误判为主机载荷求值；普通CPU控制与值求值保留自身参与范围。前缀合同可以使用命名嵌套路径，如ensures prefix(rows,count.queue.value)，沿原Ty解析到真实u32字节位置；独立形状1、实际生产来源、前缀范围与覆盖失效规则保持，值相等的另一计数字段不能替代该来源。

权限沿调用点当前阶段及实际来源交接。read可借自本阶段合法共享只读的inout来源。跨invocation普通读取与写入按同一原缓冲内实际访问的存储区域判断；声明偏移及标量宽度已确定的不相交字段可以在同一阶段分别共享读取和按当前元素独占写入，嵌套字段及真实宽／窄布局保持。固定数组及向量的动态下标保留原声明中全部合法候选位置，覆盖嵌套数组、记录字段、swizzle与位置参数转借；数据位置和write参数的初始化义务分别交接。实际重叠继续拒绝，范围证明与原来源关系保持，不能借字段独立性取得未知位置的写权。append输出／计数仍不能在本阶段作为普通共享输入读取。字段投影不因取出字段而取得其他位置的修改权。inout／write不能在函数内升级为atomic，atomic不能伪装成read。计算函数 的已有阶段接口可以转交其声明的权限，函数消费该阶段实际的访问模式，不根据最外层缓冲的初始声明猜测权限。元素／cooperative函数的write／inout／无访问模式dataset参数仍拒绝，独占写入继续由parallel的元素所有权表达；普通CPU应用的write／inout完整资源参数按前述规则处理。

普通CPU函数按原参与范围确定借用ABI：Invocation的read／write／inout／atomic dataset交接真实地址、长度、u64步长和字段偏移；Host参数交接原Frame句柄、长度、记录步幅和字段偏移四个u32；DataPayload与实际Place直接保存Access，不以可写布尔值代替原子权限。atomic参数只允许原子操作及atomic转借，不能变成普通read／write／inout或不可变view；普通可写资源可按已初始化的真实区域借给atomic参数。atomic T沿已有原生位置参数ABI交接，字段、嵌套数组与非连续向量分量保留实际地址。Atomic直接使用IndexedSource的Data／Tile／View位置，不另存目标类别和重复下标；CPU地址上的读改写使用真实LOCK指令，加载／存储保持显式内存序，GPU不解释CPU裸地址。资源仍由原Frame拥有，最后真实消费者完成后释放；普通CPU函数自身不因此创建并行域。对象合同与资源C ABI采用语言规范“已编译模块的资源ABI”中的现行编码版本。

GPU数据集绑定以同一个16字节头交接：原页数组的64位设备地址、u32物理记录步幅、u32逻辑字段偏移。原根在既有页表分配的开头持有其零偏移头，页数组紧随其后；非零字段偏移只由本次提交参数持有另一个同格式头，指向原页数组，不复制页地址或字段载荷。B发射器按物理步幅计算SPIR-V整记录分页，再叠加字段和分量偏移。动态数组字节偏移加到64位设备地址。64位物理标量沿一次选页所得起点生成两个相邻u32指针，按低字在前拆合原值。归约、扫描、排序和临时摘要同样消费实际绑定布局，不再转传未被消费的固定stride。不可变view携带实际元素步长访问原页数组，设备指针指到原16字节dataset绑定头之后；24字节view值不替换该绑定头。集合首遍与根写回交接原逻辑描述符，实际中间层只替换自己的临时引用，不从根分配重建零偏移引用。输入根只交接一次，字段类型、初始化、read／write／inout／atomic与真实区域关系保持，DomainBindings在原require／completed处消费实际slot与offset，参数和资源由原Execution持有至真实GPU完成。Frame同步GPU调用保持等待本次完成并返回同一根，结果不隐式下载。GPU绑定头遵守现行执行映像语义；Frame共同C签名与对象合同分别按其实际编码版本交接，不相容产物须重新编译。

`for (element in data)` 可以遍历 read 或 atomic 数据集；atomic 游标只是现有元素位置，必须使用 atomic_load／其他原子算子，不能隐式读取或直接赋值。游标支持 ordinal、break、continue、seek，也可把当前原子字段继续借给 atomic T 参数。函数内不允许 parallel、phase 或隐式协作同步；cooperative 仍须声明至少一个 tile 参数，并从调用点继承同一逻辑块。通用只读／原子数组不会把一个值函数变成新的并行域。

固定 N 参数要求调用方声明完全相同形状；动态形状参数可接收固定或动态来源。独立库检查按自己的接口证明索引，不能依赖某个入口恰好提供的长度：例如动态 dataset 的普通索引参数必须由接口 index 约束，或在函数内通过实际长度条件／取余／有界游标证明，不能直接读取任意 i。长度与索引关系属于实际数据集，不能把另一数组的范围移过来；独立动态函数也不能无声明地转交给要求固定 17 项的函数。

借用覆盖完整容量，不隐含一个 Count 前缀。prefix 的 requires／ensures 仍由 计算函数 接口管理；普通／cooperative 函数的值与索引 requires 见下一节。函数可原子修改实际 Count，但其写效果使后续旧 prefix 关系失效，不改变已捕获的当前循环域。这里没有新加运行期边界检查或 Count 读回。只读／原子函数不进入要求普通值参数的归约／扫描回调；纯集合合并器仍不能隐藏资源副作用。

整份dataset借用沿类型、形状、边界、初始化、原子权限及目标合同检查；普通CPU应用的整份read／write／inout见应用章节。可写view、第一类通用资源引用、通用资源泛型、动态调用及完整并发队列仍待实现。

## 独占位置的 inout 函数借用

普通函数与 cooperative 函数可以接收 `inout T`。实参是调用方已初始化的独占位置：当前 parallel 元素、其字段／无重复分量的 swizzle，或可变局部变量及其字段。参数直接使用原来的局部槽或全局缓冲 word 映射；不会在调用入口复制整个 T，不创建运行时引用对象、引用 ABI、宿主回调或设备任务。函数可继续把整位置或互不重叠的字段借给其他函数。

`.h` 示例：

```c
module motion;
struct BLBody { float3 position; float3 velocity; f32 energy; };
f32 advance(inout float3 position, inout float3 velocity, f32 dt) on compute;
void step(inout dataset<BLBody> bodies, f32 dt) on compute;
```

对应 `.b`：

```c
module motion;
f32 advance(inout float3 position, inout float3 velocity, f32 dt) on compute {
    velocity += float3(0.0,-9.8,0.0) * dt;
    position += velocity * dt;
    return 0.5 * dot(velocity,velocity);
}
void step(inout dataset<BLBody> bodies, f32 dt) on compute {
    parallel (body in bodies) {
        body.energy = advance(body.position,body.velocity,dt);
    }
}
```

同一次调用的所有 inout 实参必须互不重叠：两个不同字段可以借用，整个记录与自身字段、相交 swizzle、同一位置的不同写法均拒绝。按值实参保存求值时的快照，可以来自被借位置；实参与索引从左到右求值一次，所有实参求值完毕后执行函数体，借用结束后调用方继续使用原位置。`bump(value,bump(value,2))` 的内部调用先完成，外部调用再使用更新后的 value 和已捕获的增量，不会重算索引或恢复旧内容。

局部绑定若只通过函数修改，也由同一写效果分析确定其可变存储。读取参数得到普通 T 值，`return value` 返回快照；参数本身不是可存入记录或逃逸的引用。普通按值参数仍不可直接作为 inout 实参，可以先声明自己的局部副本。inout 不授予新并行域、任意散写、只读／原子权限升级或块内共享写；只读 view 的元素及只读／atomic tile 不能作为 inout 位置。

普通值临时数组的元素及其普通字段可沿原实际位置交给write／inout。数组返回值先进入当前执行的独立局部存储，修改只作用于该临时副本，不回写函数的按值输入。CPU位置及其地址沿本函数帧和既有来源／逃逸规则存活；可在该帧内继续消费，不能从函数返回或输出局部地址。GPU继续使用原阶段局部存储，不把设备位置当作CPU地址。按值参数、不可变绑定及read来源在原绑定入口保持既有只读权限，不能借物化或取址升级。新存储、初始化、Proof、ABI及释放职责沿原所有者，不增加拥有表、写回镜像或第二释放链。

write dataset 的字段必须在借出前已初始化；可以先初始化一部分，再借出这部分，不必读取其他字段。独占元素和局部位置共用一份逐分量来源、范围、资源域及非零信息，直接写入和函数写入在同一位置更新这些事实。当前独占字段的重复装载可使用同一值身份，写入后只代表新内容；只读外部数据和原子内容的重复装载等价仍未完成，原子读取不能当作稳定快照。

if、for、while、短路与三元式中的函数写入进入原有控制流效果。函数返回的 bool 可以约束调用方当前内容；旧快照的条件不会约束改写后的内容。普通 if、短路与三元式按实际静态字段／分量合并身份、数值区间、索引域和非零事实，不因只改写一个字段就清空整个记录；循环入口只撤回可能被条件或正文改写的分量，未写字段保留原事实。函数位置读写沿原 IR 进入值转发、活跃性、阶段依赖与 Count 关系失效；cooperative 中的 inout 仍是各 invocation 的私有位置，不增加屏障或共享存储。

局部记录中的不可变 view 句柄可以整体交换或替换，再按新句柄读取；实际子资源继续由原闭合图持有，不能修改 view 的地址分量或子元素，也不能把含 view 的记录写入可写全局数据集。普通值／字段的nonzero／index约束仍只接受按值参数，不能对可变位置声明这些输入条件；普通CPU dataset的显式共同元素条件分别按[共同值](#common-element-nonzero)、[共同资源](#common-element-nonempty)及[同一元素索引关系](#common-element-index)交接。

借用冲突、重复分量或不具备独占权限报 B227；未初始化沿用 B142、类型 B124、索引 B137、非当前元素的可写访问 B166，目标 B161、递归 B160、签名 B028 保持。重复参数名由声明解析统一报 B100，不能在静态内联时覆盖位置。`write T` 输出借用与 void 函数见下一节。普通CPU完整dataset的read／inout泛型借用沿实际资源交接，临时聚合的dataset字段同样使用原位置；write T沿本次真实输出初始化，不继承调用前旧值。第一类引用、动态任务和其他资源泛型继续沿Issues具体范围推进，独立F保存的读取借用按原最后消费者保持，资源所有者F继续直接接管其拥有载荷；多元素借用按实际选择来源区分。现有 `simulation.bodies` 已以两个互不重叠的 float3 位置封装无返回值积分，以 write f32 位置输出能量；独立 integrate／energy 入口保留。

## void 函数、write 输出与确定初始化

`void` 表示函数没有返回值，只能用于普通／cooperative 函数的返回声明，不能成为变量、参数、字段、dataset 元素、view 元素或普通计算值。普通 void 函数可以自然结束，也可以在分支、顺序循环或末尾 `return;`；有返回值的普通函数可在这些位置返回匹配类型，所有继续路径必须返回。cooperative 函数仍限制为末尾返回，不能从 parallel 内 return。调用作为语句执行，写效果照常保留；把 void 调用绑定到 let、用于运算／条件或其他值参数报 B228，不用占位零值表示无结果。

`write T` 借用一个完整输出位置，不消费调用前的内容。与 inout 相同，它只能来自当前独占元素／字段／无重复分量的 swizzle，或可变局部位置；同次所有 write 与 inout 参数必须互不重叠。未声明末节initialized条件的函数，必须在结束前完整初始化每个 write 参数的全部逻辑分量，漏写报 B229；写前读取、复合赋值或转交给 inout 报 B142。写出某个分量后可以读取或继续修改该新内容。输入内容即便在调用方已初始化，也不能作为 write 参数的旧值被函数读取；这里撤回的是编译期知识，不是向实际缓冲区填零。

调用方可用 `T name;` 声明未初始化的局部记录、向量、标量或不可变 view 句柄。它只声明实际局部位置，不生成默认值或清零指令；读取前必须确定所需分量已写入。`let name = expression;` 仍必须有初值。字段可以逐个写入或借给输出函数，完整读取记录时才要求全部逻辑分量就绪；ABI 填充不是要额外填满的字段。普通值实参与位置索引仍从左到右求值一次，在所有实参求值之后开始输出函数；因此已初始化的 x 可以用于 `assign(x,x+1)` 的输入快照，未初始化的 x 不能借此读取自身。

`.h` 示例：

```c
module outputs;
struct State { uint4 values; u32 checksum; };
void generate(write uint4 target,u32 seed) on compute;
void run(write dataset<State> states) on compute;
```

对应 `.b`：

```c
module outputs;
void generate(write uint4 target,u32 seed) on compute {
    target.xy = uint2(seed,seed+1);
    target.zw = uint2(seed+2,seed+3);
    return;
}
void run(write dataset<State> states) on compute {
    parallel (state in states) {
        uint4 values;
        generate(values,index());
        state.values = values;
        state.checksum = values.x ^ values.y ^ values.z ^ values.w;
    }
}
```

普通有返回值函数也可以带 write 参数，返回值与输出位置各有自己的职责；输出仍须在函数结束前完整写入。多层输出转交、不同字段、swizzle 和局部含 view 记录使用同一个原位置映射，不分配引用对象、不复制整个输入记录。view 输出只构造不可变句柄，实际子资源继续由原资源图持有；不能修改句柄的地址分量、子元素或把含 view 的记录写进可写全局数组。

确定初始化扩展原有的 `Builder.initialized`，以实际局部槽／独占元素 word 为键；没有第二份输出状态或运行期初始化位。if 与三元式从同一入口状态分别检查两侧，仅保留都确定初始化的位置；已知常量条件选择实际执行侧。&&／|| 只在右侧必定执行时把其新增初始化带出。while 条件在循环退出前至少执行一次，其确定写入可以带出；仅在循环体中的写入不能证明零次循环后已初始化。for 的起止表达式先执行，循环体仍可能零次。分支／循环中的值来源与数值范围合流仍有既有保守限制，不把初始化完成当作任意数值条件已成立。

cooperative void 函数可以在协作根块用 `cooperate name(tile,output...);` 直接作为语句调用；也可多层转交同块 tile 与私有输出位置。原 tile 集合仍在根块 let 声明中产生，发布／同步作用域保持，不把 write 位置变为跨 lane 共享存储。普通 void 函数不能隐藏屏障。归约／扫描的回调仍需普通返回值，不能用无返回值函数充当数值算子。

静态内联沿用现有 Load／Store／LoadLocal／StoreLocal 与阶段读写集合，直接声明没有默认初始化指令，void 没有结果寄存器。函数写 Count 同样使后续旧 prefix 关系失效。CPU 应用整份 write dataset 的当前完整初始化范围、已初始化 inout 借用及普通函数提前返回已实现；一般动态子域／数组初始化推导、其他资源种类返回、cooperative 提前返回与设备自主任务仍未完成。

## 普通函数的提前返回

`return value;`／`return;` 结束当前普通函数。它们可位于 if、while、for 和多层局部循环中；函数调用完成后，调用方继续执行。break 只退出当前顺序循环；continue 跳过当前循环体余下语句，for 先将最内层游标加一，while 直接重新求值条件；seek 把最内层 for 游标置为指定位置并重新检查，不额外加一。三者都不能控制调用方的循环或直接退出 parallel 实例。

编译器按每条真实返回边检查返回类型与 write 输出完整性，再合流返回值和借用位置的当前内容事实；一致性包含所经分支和循环范围。void 的自然结束也是返回边。提前返回的一侧不参与后续语句的初始化与范围合流，例如 `if (position >= source.length) { return 0; }` 后面可以读取 `source[position]`；`if (divisor == 0) { return 0; }` 后面可以执行整数除法。输出缺字段报 B229，读取尚未初始化位置沿用 B142，有值函数缺少返回路径报 B132／B133。

顺序块合成 return、break、continue、seek 和顺序继续的边；循环后只合流条件失败与 break 的真实状态，不把函数返回当成循环继续。若 while(true) 内部分支 break，函数仍需处理这条循环后路径。已知常量条件只生成实际路径，已知空 for 不编译其循环体，但起止表达式仍按顺序求值，while 条件的实际副作用也保留。完整模块仍先做独立函数检查，不依赖某个调用恰好使用常量才接受非法的通用函数。

`continue;` 可用于普通函数、CPU 应用、parallel 实例或 cooperative 函数内部的局部 while／for。范围、dataset、view 和 tile 的 for 都沿同一个游标推进规则；嵌套循环只推进最内层，不跳过外层循环。函数的循环上下文与调用方分开。计算函数 阶段层的静态 for 展开不属于局部顺序循环，不能在阶段层使用 continue。

```c
u32 odd_sum(u32 limit) on compute {
    u32 total = 0;
    for (i in 0..limit) {
        if ((i & 1) == 0) { continue; }
        total += i;
    }
    return total;
}
```

continue 与 seek 共用原有 IR 循环回边，不产生新的调度任务或运行期状态。离开本轮循环体时，释放本轮及嵌套块内拥有的局部 dataset，保留循环入口及调用方资源；显式 CPU／GPU 存储转移仍须在回边前配对完成。continue 路径不成为循环后的出口，不将该路径上的局部初始化或条件事实误带到循环后；cooperative 的同步与发布仍只允许原有根块位置。

例如 `.h`：

```c
module search;
struct Sample { u32 key; u32 value; };
struct Query { u32 key; u32 value; bool found; u32 audit; };
void find(read dataset<Sample> source, u32 key,
          write u32 value, write bool found) on compute;
void search(read dataset<Sample> source,
               inout dataset<Query> queries) on compute;
```

对应 `.b`：

```c
module search;
void find(read dataset<Sample> source, u32 key,
          write u32 value, write bool found) on compute {
    for (sample in source) {
        if (sample.key == key) {
            value = sample.value;
            found = true;
            return;
        }
    }
    value = 0;
    found = false;
}
void search(read dataset<Sample> source,
               inout dataset<Query> queries) on compute {
    parallel (query in queries) {
        find(source, query.key, query.value, query.found);
    }
}
```

元素／cooperative 函数继续静态展开到同一 CPU／GPU IR，CPU 应用普通函数在自己的原生帧内降低同一控制流。只在需要提前退出时建立函数 Scope；x64 直接跳到该函数域末尾。SPIR-V 的普通分支、循环和函数作用域由 B 发射，循环返回使用原实际前驱的 SSA 退出值离开当前函数域。不增加宿主回调、逐语句返回标记、默认输出清零或新的执行循环。常量特化后若只剩单一末尾返回，退出域会被消除。

普通函数返回在自身域合流，父级块内屏障由调用方显式执行。cooperative提前返回报B208，整块一致退出、动态协作和控制流内屏障仍未实现。never函数结果已按同名章节支持，一般never数据组合、完整循环不变式及任意细粒度字段关系仍待实现。不可达源码仍作语法解析，完整不可达类型诊断尚未提供。编译成功不保证终止或业务正确。

bool 的多个返回点按 false／true 两种结果分别合并。每个返回点从当前环境取得原值身份上的 u32 数值区间、资源索引域及 u32／i32 非零事实，与返回表达式自身的条件共同交付；只能产生某一种结果的常量返回不参与另一种结果的合并。同一结果的全部可能路径取共同事实，继续使用原布尔组合规则。例如：

```c
bool inside(u32 i, read dataset<Sample> source) on compute {
    if (source.length == 0) { return false; }
    if (i >= source.length) { return false; }
    return true;
}
```

调用方在 `inside(i,source)` 成立的路径可以读取 `source[i]`；同样的返回方式可以证明除数非零，也支持向量／记录中的既有逐分量事实。事实仍绑定原值和原资源，不能约束改写后的索引或另一个数据集。存在没有建立索引关系就返回 true 的路径时，该结果不能证明读取安全。函数仍生成原 Scope／Exit、比较和控制流，没有运行时校验、额外状态或通用算法正确性证明；缓存谓词的循环回边和更一般的符号关系保持未完成。

## 动态数据集的同长合同

`requires same_length(first,second)`声明本次调用入口两个dataset形参的真实元素数相等；元素类型、物理存储和权限仍各自保持。合同是.h／.b签名的一部分。零长合法，长度也可以在每次执行时变化；同一个类型／const实例消费不同实际N，不因N复制代码。

```c
void split(inout dataset<Receiver> first,write dataset<Signal> second)
    on compute requires same_length(first,second) {
    parallel (item in first) {
        item.value += 1;
        second[index()].value = item.value * 3;
    }
}
```

同一长度关系沿实际Proof、整域接口、ProgramLayout和执行映像交接；不同形参名、传递顺序和相等关系链仍指向同一长度事实。普通requires index直接消费Data索引域的既有等长关系；dataset.length或DataLength仅是长度，不能据此成为有效索引。编译期可由固定相等形状、同一长度值分配、当前路径已证明的长度相等或上层same_length前提交付。普通CPU函数的原生调用、GPU target和已编译模块各在原调用点证明要求；未知或不等长度报B226，不能仅因一次实际数据恰好相等而默认前提。CPU应用直接parallel捕获实际数据集时，编译器把已经证明的同长关系交给其原提取入口。

独立整域入口没有B调用方时，由原运行库参数绑定入口检查实际长度关系；CPU与GPU共用该入口和原长度参数字，调用开始前检查一次，失败不执行阶段。Frame仍拥有原资源，字段投影按原父记录数量交接。没有逐元素长度判断、长度副本、资源复制或新的完成链。合同只交付相等容量；read／write／inout／atomic、当前index独占和写入初始化继续按真实参数及阶段检查，相等长度不授予任意散写、存储别名或有效前缀。

本谓词当前用于dataset形参的requires，不声明非空或函数后置条件；其他资源与一般尺寸表达式按其实际能力另行建设。公开长度关系变化按原模块合同比较失效，模块调用消费已编译关系，不读取提供者实现。ProgramInterface显式交接length_requirements，旧实现须更新该公开接口；实际对象／映像编码集中见“已编译模块的资源ABI”。

PrefixRelation的domain_region为None时表示该实际绑定的完整声明类型；Some(offset,bytes)表示同一原记录内的实际字段区域。原ProgramLayout、ExecutionPlan与DomainBindings按真实偏移交接，要求的区域必须由已生产区域覆盖，并保持相同实际Count字段。静态Count事实与整域接口输出编码为对象格式；上述执行合同编码为执行映像格式，旧格式须由原源码重编。Frame、物理存储、CPU／GPU完成及最后消费者的释放链保持原所有者。

## 函数参数的编译期约束

普通函数与cooperative函数可以在`on compute/cpu/gpu`后声明requires。它是.h／.b签名的一部分；B调用点证明真实前提，被调用方使用同一前提精化原参数Proof或真实Count存储模型，随后沿实际目标的原生调用或内联交接，不因约束生成逐元素检查。独立整域入口的同长资源合同另按上节在原参数绑定边界消费。

| 约束 | 含义与适用参数 |
|---|---|
| `requires nonzero(value)` | 按值整数标量／向量的每个选中分量非零，或cfn函数地址非空；支持记录字段和 swizzle；普通CPU按值拥有聚合的字段及owned包装的value沿同一规则，不表示有符号数一定为正或代码签名／寿命已经证明 |
| `requires nonzero(input[*].field)` | 普通CPU及整域dataset每个已初始化元素的所选整数分量非零；整域使用原read／inout字段前提。普通CPU的记录内dataset及嵌套元素显式按[*]选择，调用点消费同一实际共同Proof，见下节 |
| `requires nonempty(resource)` | dataset／tile、固定array或按值不可变view的长度至少为一；支持普通CPU按值拥有记录的dataset字段及记录中的view字段 |
| `requires nonempty(input[*].field)` | 普通CPU dataset每个已初始化元素中所选子dataset／固定array非空，沿同一实际长度／Proof交接；见[共同资源前提](#common-element-nonempty) |
| `requires prefix(data,count.field)` | 普通CPU函数的真实资源前提；data与count为命名dataset形参，count为独立固定形状1的可读u32计数字段；Count写入撤回关系，数值相等／复制不建立前缀，实际参数可为字段投影 |
| `requires contiguous(position)` | write／inout普通位置参数或其字段／swizzle按所选类型连续、顺序匹配且对齐；不读取载荷 |
| `requires range(value,lower,upper,...)` | 所选数值分量属于类型化闭区间并集；普通按值、共同元素与整域输入沿原范围合同交接，不授予资源访问或输出保证 |
| `requires index(value,resource)` | 按值u32标量或向量每个选中分量都可索引该实际资源；同时可知资源非空，不表示索引互异 |
| `requires index(input[*].index,input[*].values)` | 普通CPU同一已初始化元素的u32分量可索引其实际子资源；沿原资源身份与长度Proof，见[共同索引关系](#common-element-index) |

`.h` 示例：

```c
module function_requirements;
struct Job { u32 value; u32 divisor; u32 result; };
u32 quotient(u32 value, u32 divisor) on compute requires nonzero(divisor);
void run(inout dataset<Job> jobs) on compute;
```

对应 `.b`：

```c
module function_requirements;
u32 quotient(u32 value, u32 divisor) on compute requires nonzero(divisor) {
    return value / divisor;
}
void run(inout dataset<Job> jobs) on compute {
    parallel (job in jobs) {
        let divisor = job.divisor;
        if (divisor != 0) { job.result = quotient(job.value, divisor); }
    }
}
```

独占字段的重复装载现按本次内容关联；只读外部数据／原子内容的重复装载等价仍未完成。示例显式保存本次 divisor 值，条件与实参使用同一快照。不能因另一次 atomic_load 非零就认定后来的读取也非零，也不能对 atomic T 位置本身声明 nonzero。浮点、bool、整个普通记录不属于当前 nonzero 类型，记录中的整数向量字段可以使用 `nonzero(config.divisors.yx)`。

普通字段路径选择形参及其字段；nonzero／nonempty／index中的`[*]`选择普通CPU dataset已有的共同元素模型，不执行函数调用或运行时元素读取。`index(query.indices,table)` 支持向量各分量，`index(query.pos,query.nodes)` 可以关联按值记录里的不可变 view，或普通CPU按值拥有记录的dataset字段；`nonempty(query.nodes)` 给同一实际资源的[0]／length-1提供前提。所选字段可以嵌套，约束不复制到兄弟字段或另一份资源。只读／原子 dataset、tile 的访问模式、目标、字段类型与生命周期仍由原接口负责。index 不授予独占写入、任意散写、原子升级或块内同步，也不把完整容量当作 Count 有效前缀。

所有要求都必须在调用点已经成立；数值常量、固定形状、当前范围／长度条件、已求值实参自身持有的零／非零条件、既有索引能力及已声明的上层 requires 可以提供证明。nonzero沿当前路径消费选择表达式或返回值的原分量条件，要求全部逻辑分量同时非零；未知分支、真实零分量及空函数地址不能取得非零许可。函数独立检查时使用自己的接口，不依赖某个入口实际传来的数值。前提可在多层函数中转交，修改局部副本会按原赋值规则清除相应知识；requires 不会让后续的新值继续满足旧要求。类型检查不扫描输入数据；共同元素nonzero、nonempty与index只消费已有共同事实，任意业务属性、其他量化条件与通用关系求解器仍未实现。

多个 requires 的顺序与重复项不影响签名匹配，`.h/.b` 的谓词、参数路径必须一致；语义相似但书写不同的谓词不自动视为同一签名。声明不能引用局部变量或任意表达式。普通／cooperative 函数当前不接受 ensures，prefix 约束继续只用于 计算函数；计算函数 当前也不接受这三种值约束，宿主原始输入不能被隐式假定为满足前提。

普通值集合回调可以声明这些要求，但仍须在实际集合调用处证明。例如投影的 divisor 通过 `bind(project,7)` 传入时可以满足 nonzero；未知输入元素不会因为实际数据恰好非零就取得该能力。原纯值类型、统一捕获与禁止隐藏资源副作用的规则保持，requires 不绕开集合接口。

非法声明／不支持的约束报 B225，调用点缺少证明报 B226；签名不一致继续报 B028，函数中使用 prefix 继续报 B192。body 内改写后失去普通索引／非零证明时，仍按原 B137／B201／B152 报错。本节实现上述明确输入约束；同长资源前提按其独立章节交接，不代表已有第一类 NonZero 类型、任意精化类型、通用函数后置条件、统一值入口条件或完整语言。

<a id="common-element-nonzero"></a>
### dataset共同元素的nonzero

普通CPU函数可用`requires nonzero(input[*].field)`声明每个已初始化元素的所选整数分量非零。`[*]`明确选择该dataset的共同元素模型；元素本身为整数时可写`nonzero(input[*])`，记录字段／owned载荷／swizzle沿原字段规则选择。dataset在记录字段中时可写`nonzero(input.rows[*].value)`；嵌套dataset可逐层选择，如`nonzero(input[*].rows[*].value)`。每一层保留自己的元素及资源来源，不将兄弟字段或另一份dataset的事实混用。同一当前读取合取共同数值时，非零与区间、关系及条件一起进入该值；不要求把nonzero改写成range，初始循环及return／break也不清除未写字段的非零。真实写入仍撤回被覆盖字段的旧前提；写前保存的普通值快照保持原事实，写后的新值不能回借它。

整域函数也可对普通`read`／`inout dataset`输入声明`requires nonzero(input[*].field)`；整数元素本身使用`nonzero(input[*])`。根为实际dataset参数，嵌套普通记录字段和向量swizzle按实际类型选择，沿原整数位宽逐分量排除零，不要求有符号值为正。此处与整域`range`共用同一字段区间、源码／已编译接口及真实执行前提，不保存另一份非零状态；不因该谓词取得输出初始化、访问或资源寿命。原子并发输入、write未初始化输入、浮点／Bool或整个普通记录不能作为该非零整数字段。实际字段改写后按原写区撤回旧前提，不能由兄弟字段的声明取得本字段非零。

外部生产者可沿现有可信字段交接提供非零区间：例如宿主实际以NonZeroU32生产block字段，在真实写入完成后用原DomainBindings::produced_element或Frame::complete_field交出该字段的非零范围，再由原上传及执行绑定消费。任意原始字节不自动得到该前提，缺少担保或已交出的范围包含零时，原绑定入口按既有字段合同拒绝，不扫描载荷、钳制分母或补造运行分支。该字段能力不替代自动parallel捕获uniform和一般统一值入口的条件交接，真实缺口继续见Issues C-021.1.4.1。

```b
void dispose(u32 value) on cpu requires nonzero(value) {
    if(!println(value)) { return; }
}
owned<u32,dispose> make(u32 index) on cpu { return own(11,dispose); }
void consume(dataset<owned<u32,dispose>> input) on cpu
    requires nonzero(input[*].value) {}
```

调用`consume(dataset<owned<u32,dispose>>(2,make))`时，调用方从真实生成结果的共同Proof证明前提，函数入口在原Data.elements中接收同一条件，最后静态F仍检查自己的nonzero要求。真实零值或写入后无法继续保证所有元素非零时，在实际调用处B226；没有该前提的普通consume不能隐式取得F的要求。已证明实际元素数为零时，空集合满足共同元素前提，却不提供`input[0]`访问；读取仍需独立的nonempty或index事实。只有运行时恰好为空、编译时范围没有证明时，不能借空集合跳过未成立的前提。

普通CPU函数将实际按值或read／inout参数捕获为新空dataset的唯一新增拥有元素时，原参数分量、子资源长度及同元素索引关系沿同一共同模型交给调用者；泛型、已编译对象与归档保持该关系。参数真实改写后只保留当前快照，原非零／长度事实不能继续授权。增长前后所有仍存元素的共同nonzero、子资源nonempty及各自同一子资源index关系沿原整元素事实和零／非零条件交接，连续普通调用、泛型及无实现源码的对象／归档保持这些关系。原外集合已证明为空时没有旧元素参与合流；新元素仍须独立满足前提，未知外长度保留旧元素可能存在的路径。真实零值、空子资源、越界、错资源及未知输入不获许可。共同非零不表示不同元素数值相等，元素模型的局部身份不取得函数实参的无关数值范围。普通函数按实际bool返回或用选择表达式选出一个完整值时，条件沿记录、固定数组、活动变体、dataset长度及Data.elements交接，调用者可以由本次实参证明所选输入的共同nonzero、nonempty及同一子资源index。未知选择仍要求每个可能输入满足相应前提；各输入的数值不因选择而相等，未选中拥有值按原生命周期释放。选中可证明的外空按全称空集合满足共同前提，不产生元素或下标许可。普通整数的零／非零条件按实际类型及实参交接；零分支真实可达而非零分支不可达时恢复零值，不把非零猜成某个具体数值，也不改变浮点位模式；多输入返回保留父dataset及子dataset各输入共同成立的长度范围，沿原输出合同及本次实参交接。

在原类型与借用能力范围内，该选择用于普通CPU dataset的按值、read或inout输入，以及相应记录字段；字段列借用按原projection和真实布局选择，函数内写入与调用写入效果照原规则撤回旧事实。write旧内容及atomic内容不成为这项稳定数值输入。普通值／字段不含`[*]`时保持原按值规则；C ABI、cfn和GPU program没有新增共同元素元数据入口。接口中的相同路径沿.h／.b及已编译对象／归档保存；选择不产生运行时扫描、判空、另一元素对象或新的合同权威。共同子资源nonempty见[独立规则](#common-element-nonempty)；同一元素的值—资源index关系见[独立规则](#common-element-index)。

<a id="common-element-nonempty"></a>
### CPU dataset共同元素的nonempty

`requires nonempty(input[*].field)`声明每个已初始化元素中的所选子资源长度至少为一。dataset字段与固定array沿真实类型／资源元数据选择；array本身作为元素时可写`nonempty(input[*])`。字段可以在owned的value中、嵌套记录中，或经多层`[*]`选择，例如`nonempty(input.rows[*].value.values)`、`nonempty(input[*].rows[*].value.values)`；兄弟字段或另一份资源的非空不构成所选资源的证明。

```b
struct Item { dataset<u32> values; };
void dispose(Item input) on cpu requires nonempty(input.values) {
    if(!println(input.values[0])) { return; }
}
owned<Item,dispose> make(u32 index) on cpu {
    let values=dataset<u32>(1);
    values[0]=81;
    return own(Item(values),dispose);
}
void consume(dataset<owned<Item,dispose>> input) on cpu
    requires nonempty(input[*].value.values) {}
```

调用处从实际共同Proof与长度证明前提；入口在同一Data.elements模型中接收条件，拥有字段的值元数据及其原payload长度保持一致，真实静态F仍按自己的接口使用该资源。静态F可生成嵌套固定拥有数组与活动enum／Result，混合宽窄标量和浮点字段按声明布局交接；零长度外集合不执行F，也不生成或释放任何元素。固定array的数量来自原类型。实际空子资源在受约束调用处B226，普通函数不因F声明nonempty而默许它成立。已证明外集合为空时全称前提成立，不生成元素，也不允许`input[0]`；访问外dataset仍需其独立nonempty或index事实。只有运行时恰好为空、编译时未证明时，不能借此省略前提。零长度array的共同非空前提等价于其直接外dataset为空；入口沿原Data.length与拥有父值中的同一长度Proof接收该条件，因此可继续转交相同前提。它不产生array元素，也不放宽元素0的访问；实际改写后仍按当前资源重新证明。

前提可经普通函数、泛型、记录及无实现源码的对象／归档交接。当前合法的read／inout记录参数也交接同一拥有树；整体替换资源后，后续调用须按替换后的真实长度重新证明，改写无关字段不借走或制造资源事实。选择没有运行时扫描、判空或额外合同状态，C ABI、cfn与GPU program没有新增共同元素入口。普通标量／资源路径不含`[*]`时保持原规则。

拥有元素dataset可进入普通CPU的read参数，以原真实借用读取length及普通标量字段，支持原同根只读别名、转发与字段布局。合法元素位置可交给普通CPU的read拥有聚合形参，原子资源元数据／载荷及返回view／地址保持真实来源与父拥有寿命。命名动态元素的子dataset可直接读取长度，并在原范围及位置合同成立时索引或构造view／地址；同一CPU执行段、本轮while条件到正文，以及实际写效果不覆盖读取且地址和下标稳定的while／for，都可沿实际装载数值来源使用已证明的直接长度；无拥有元素的写入可按不同真实分配，或同存储已知不相交的元素／字段／固定数组字节位置，保留既有读取事实；有写普通调用实际参数位置的循环精度仍见Issues C-005.13.2.2.5.2，普通inout入口与写回继续按实际合同交接；整元素、记录／固定数组路径及跨子dataset的拥有元素和字段移动替换按后文实现；动态集合混合CPU／GPU元素的位置由原模型及实际下标选择条件交接；含view元素的构造与当前细化缺口见[CPU拥有元素的dataset](#cpu-owned-dataset-elements)及C-020.1.6.2.7。同一共同元素值—资源index关系见[索引前提](#common-element-index)。本节的子资源前提不代替这些独立存储／访问能力。

<a id="common-element-index"></a>
### CPU dataset共同元素的index

`requires index(input[*].value.index,input[*].value.values)`声明每个已初始化元素中的所选u32分量都能索引同一元素的实际子资源。两条路径必须选中同一dataset、同一嵌套元素作用域；记录内dataset及多层`[*]`沿同一规则选择。u32向量和swizzle逐分量交接，未选择的分量不取得前提。当前子dataset与固定array按原合法参数、元素类型及访问模式使用。

```b
struct Item { u32 index; dataset<u32> values; };
void dispose(Item input) on cpu requires index(input.index,input.values) {
    if(!println(input.values[input.index])) { return; }
}
owned<Item,dispose> make(u32 i) on cpu {
    let count=(i&1)+1;
    return own(Item(count-1,dataset<u32>(count)),dispose);
}
void consume(dataset<owned<Item,dispose>> input) on cpu
    requires index(input[*].value.index,input[*].value.values) {}
```

调用处使用同一共同元素的实际值、资源身份与长度Proof；例如上面的长度在一至二变化，长度减一与该实际长度的关系仍可交接，不要求所有元素长度固定或用最小长度代替关系。可证明恰好新增一个拥有元素的resize及原sequence::push保留旧、新元素分别对其实际子资源成立的关系，连续增长继续交接。已证明原外集合为空时只接收实际新增元素；空集合不为非法新索引提供前提，未知外长度也不被当作空。入口在同一Data.elements的原值Proof标记原资源IndexDomain，并沿原拥有树接收所选子资源非空；静态F仍检查自己的接口。改写索引或整体替换资源后，后续调用按当前内容重新证明，不能沿旧句柄或另一份资源继续使用前提。记录、泛型、多层普通函数及移走实现源码后的对象／归档均交接这些原事实。

已证明外集合为空时，全称前提成立；零长度array上的共同index前提只在其直接外dataset为空时成立，可沿原长度Proof继续转交，不产生元素或元素0许可。只有运行时恰好为空、编译时未知时仍须证明前提。真实越界或错误资源在受约束调用处B226，缺少独立外dataset下标事实的读取仍B137。

普通CPU共同index也可把所选共同u32字段关联到独立全局dataset、不可变view或固定array，例如`requires index(input[*].index,target)`。它表示每个实际已初始化源元素都能索引同一目标，不是两个集合逐项配对；向量／swizzle及嵌套源作用域沿原规则选择。源作用域为空时前提成立，但不能由此推出目标非空；原Clause表达“源作用域非空才推出目标非空”，声明先后不改变逻辑。普通字段写入、目标存储版本及实际调用仍沿原失效规则；普通循环／多层只读遍历、空源及独立obj／lib继续交接同一前提。嵌套游标写无关字段仍有过度失效缺口（Issues C-021.1.4.5.4.1.1），命名游标替换子资源沿同一实际Place交接；普通函数循环中用静态工厂或直接dataset构造替换资源字段、连续替换、写嵌套字段、整元素替换及显式下标更新时，已有子长度可通过原元素及输出模型交给调用者；新子资源的初始化不修改父记录中的资源描述，真实清空与跨下标覆盖仍按实际写入合流。实际选中条件写入的已有数值事实随原循环与普通written合同交接，调用方仍须证明其真实选择条件。普通分支中的dataset与拥有记录按实际选择保留已有数值关系，未写分支继续使用原输入事实，真实零值或空子资源按所属分支检查；完整for的自然出口与continue按实际路径交接已写值和未写分支的原输入关系；真实清空、其他下标写入及变化条件继续影响共同保证，不能由一次选中写推定所有路径。显式下标循环的普通inout调用在原IR已证明全部相交引用写指向当前实际位置／下标时，沿同次调用的ParameterStorage位置、原字段布局及已绑定written合同交接对应分量；循环别名只经原拥有作用域查询，不依赖可选观察来源或重建参数ABI。未知写、其他下标写与真实清空继续影响保证。普通CPU无seek的dataset命名游标遍历，在部分遍历及break出口把原入口未遍历部分与实际继续／break写值的共同数值交给原元素和written合同；只发布所有可能元素共同成立的事实。完整自然出口保留已证明的覆盖范围，真实清空及跨下标干扰仍参与合流，不把break当作完整遍历。两个独立共同集合的逐项配对及标量对共同子资源的量化映射仍未定义，报B225，不隐式zip或笛卡尔组合。普通不含`[*]`的index保留原按值u32／向量规则。接口没有新增运行时扫描、检查、索引对象、合同状态、C ABI／cfn或GPU program共同元素入口；拥有dataset的read／inout入口和整元素移动替换按对应章节使用，动态集合混合CPU／GPU元素的位置区分仍按其Issues编号推进；含view元素的静态F构造沿下文CPU拥有元素规则，借用精度、返回长度与完整元素赋值的剩余范围见C-020.1.6.2.7。

### 原生整域的持久字段索引前提

共同字段的逻辑索引在实际读取时，同时消费原目标声明形状、已有same_length长度Proof和当前路径上已成立的长度上界，形成同一个数值Proof；例如目标长度为二时，字段乘八加七可索引十六项表。未知长度不猜成常量，真实写分量先沿原memory_regions撤回，空集合的全称前提不创造机器值。该静态推导复用原data_index_proof／Proof::element_facts，不增加运行检查、元数据或公共ABI。同一次完整parallel写入同长度的其他普通可写dataset时，当前元素的数值模型沿原Place与Proof更新，不依赖源码命名的迭代缓冲；真实写字段由原Stage.element_guarantees和prefix::Facts交接。稳定排序只重排完整记录；完整等长输出沿同一Facts与Stage.element_guarantees接收源记录已有的共同数值和逻辑目标。前缀或更大输出保留未写尾部，输出共同事实因此合入原目标事实；未知尾部不取得源范围。该完成沿原模块合同和ExecutionPlan交给DomainBindings，不携带单元素值身份、路径或借用，也不增加运行检查。没有输入前提的独立排序入口不能从任意外部字节取得字段保证。输入tile在阶段正文前捕获；其读取按原输入绑定、捕获字段字节基址和所选分量消费同一阶段入口element_facts。普通dataset读取继续按原memory_regions撤回当前相交写；后续源写不改变已捕获字节，新源值也不为旧捕获签发保证。CheckedTile的原来源持有实际元素描述：输入及协作参数持原Ty，发布来源持本次已求值Value，类型直接从该来源取得，不再分离保存Ty，也不另存Proof缓存或事实表。不可变发布读取按实际字段／分量投影原Value，经既有capture及element_facts交出共同数值、已知位和逻辑数据目标；值身份与局部路径不跨元素，不表示不同lane的值相等。原子tile可能已被真实写入，读取不继承初始化值保证。后续局部改写不重新求值或改变已发布值；降低后仍只交原Tile／TilePublish及原公共执行／资源ABI。

同一已求值整数来源的加、减和常量乘法比较，由原Clause共有事实查询消费已有Computed整数式、Scalar、Below及数值上下界；本次查询内按同一来源消元，并把成立的整数界留在原Facts供实际索引及分支读取。u8／i8至u64／i64分别使用原机器整数的位宽、有序编码和零位置；只在当前来源范围内不发生回绕的每一步运算才作为数学整数等式展开。会回绕、不同来源或非仿射运算继续保留原机器值，不把它们拒绝为非法B程序。旧值快照与当前改写值分别命名，等价身份沿原事实交接；局部系数及查询中间材料在本次查询结束释放，不增加公共表达式格式、持久缓存、运行状态、范围钳制或新的条件图。分支入口的原required_in已推得的整数Minimum／Maximum／Zero／NonZero，相对本次输入环境新增的部分并入该分支原path，沿原路径合流交给后继读取；同一份查询结果继续精化原Value，不新增查询、逐Bool重扫、来源快照或持久状态。真实改写继续使用新值身份，已取出的旧值快照沿原身份解释。动态选择式在原分量槽合流读取形成实际结果寄存器后，复用原条件partition及两侧已有Proof::output_aliases，把真实继续侧的分量数值与结果身份关系并入同一path；后继条件沿该原路径取得被选值的范围。两侧来源或条件随后改写不改变已取出的选择结果，结果自身改写沿新值身份；常量选择及无继续值沿原控制流处理。不增加数值查询、寄存器、条件图、公开合同格式或运行时指令。整数按位&／|／^在原observe_integer_binary消费两份已求值Proof，按实际Scalar的已知零／一位形成结果KnownBits，交给原Proof.domains及同次path事实；没有已知位的部分保持未知。&只有两侧均为一的位才必为一，任一已知零位必为零；|消费任一已知一及两侧共同零位，^只消费两侧都已知的位。原非零消费者可使用确定位中的一，普通记录、向量分量、转换、合流及对象合同沿同一Proof交接；真实改写或截断撤去不再成立的位，数值事实不授予初始化、访问、借用或寿命。区间包络仍由原算术所有者交付。原Proof::holds_range与ElementRange::within复用Scalar::covers_range，判断已有数值区间与原始固定位的交集是否满足所需区间并集及固定位；在前提区间的每个空隙和所需位的相反值中，只要仍有符合已有位的机器值就不能满足前提。固定零／一位按实际Scalar的有符号偏置进入数值次序，越过最高冲突位并回到低位的必要模式，不逐机器值枚举；64位端点之后的位置由本次局部宽整数表示，浮点继续原数值区间包含。同一ElementRange以binding、实际字节offset与Scalar命名字段，同时保存有序intervals及原始known_bits(mask,value)，含义为两者交集；不能把区间包络中的洞当作实际值。原Proof的element_ranges／element_guarantees交给Stage输入与输出，prefix沿同一字段假定及证明；对象格式对象与执行映像格式执行映像编码同一描述。完成的原生产者将该字段事实交给DomainBindings，投影、只读别名及重绑定保持原存储关系；同字段多个保证用原ElementRange::intersect求交，实际写仍按原字节范围撤回，未知及部分写不冒充完整保证。运行绑定的require用同一包含运算，不读载荷推定事实、不增加资源副本、拥有表、图或缓存。普通CPU完整写后，原ElementCompletion保存同一intervals与known_bits，既有SDK直接下降和对象资源代码生成各在blink_complete_field末尾交出known_mask／known_value两个u64；原Frame::complete_field将两者与已有区间及u32上界快照共同交回同一ElementRange。固定位只适用于原整数Scalar，未知位不产生该位保证；这些数值事实均不授予初始化、访问或寿命。本字段描述不等于任意条件编码、多层来源或原24行不同路径集合均已形成，剩余仍沿原父项C-021.1.4.4.3.6.2.1.1.3。整数商仍沿原Computed关系把商与除数的实际类型化范围反推给原被除数：无符号按floor，有符号按向零截断，零商包含两侧余数范围；动态除数只消费当前已知数值范围，正负部分分别取原像并合流。实际MIN除以负一产生原位宽低位结果，原像保留该MIN输入，不把回绕分支当不可达。所有原像均与原Scalar机器域及既有来源范围相交，只精化同一已求值来源，不改除法指令、常量折叠、非零除数许可或数据布局。公开条件投影在原Proof::at_call识别确要退出本次命名作用域的Below／Computed数值关系，在私有约束仍可取得时按原条件根去重，消费现有Clause共同事实查询，将成立的Minimum／Maximum沿原fact映射直接交到同一公开条件。完整绑定的普通调用继续原图翻译，不增加私有数值消元；原分支、析取及其余关系仍由原图交接，不把某个分支的范围冒充所有返回路径。Bool直接返回比较、取反及记录分量按同一实际结果解释，不要求业务先分支再返回常量；旧值快照、实际改写值与不同来源继续分别命名。私有算术身份与内部图不导出，不增加表达式格式、第二合同、持久状态、运行检查或数据拥有者。提供者本次编译出的同一合同可由源码、无提供者h／b的obj及标准lib消费；已丢失相关信息的旧对象须重编提供者，不能靠调用方猜回原实现。对象与执行映像格式及实际运算／资源完成保持。已发布整数值的非零位置由原生产点Value交到整数tile_reduce_sum：原整数观察事实同时保留当次已求值结果的纯数值界，发布Value沿既有条件引用原生产路径；消费点后续局部改写和源写不改变旧发布值。固定块宽的真实lane沿原Computed表达式交接全局index对块宽取模，与任意有界输入及普通块内索引许可分别解释。求和只查询同一发布分量的非零条件及真实坐标范围，对lane的常量模余数允许区间用整周期与尾段直接计算位置数上界；多个约束取已成立的共同上界，不枚举lane或扫描载荷。该数量与原有效长度和逐值数值包络共同形成数学整数总和界，再按原位宽解释模回绕，结果仍只归原Proof.ranges。记录投影和重排按实际分量取原条件；普通跨元素读取继续去掉局部身份及条件，不能把稀疏值广播后的所有位置当作原稀疏集合。未知坐标、未知参数宽度和未提供非零关系时保留原逐值求和范围，不伪造紧界；F32规则、归约指令与顺序、同步、资源和公共格式保持。查询临时材料随本次分析结束，不新增环境快照、发布Proof副本、计数表或持久状态。

机器整数移位在原算术结果Proof中交接KnownBits。左移按原Scalar位宽截断并保留实际补零位，无符号右移按逻辑补零；有符号右移只在原符号位已知时交接相应的高位填充值。位移量沿原shift_counts按同一机器位宽解释，零位移及位宽倍数不误作非零移位。每个允许位移闭区间以倍增的位交集计算其全部位移共同成立的位，多段再求共同位；计算量随位宽的对数变化，不逐输入数字或逐位移值枚举，也不新增数值阈值。

来源仍是同一已求值操作数的范围和已有位事实，结果只归原Proof／KnownBits；一次移位运算捕获一份原输入Proof供各分量消费，临时材料在原算术结束后释放。实际改写不沿旧身份取得新值保证，符号未知的填充位保持未知。源调用与无实现源码的obj／lib沿原公开数值合同交接这些位，对象与执行映像格式及原运行资源ABI保持；位事实推导本身不生成运行检查。CPU范围降低器的立即数及变量位移量均按同一Scalar位宽解释；窄变量在原count寄存器交出实际低位，32／64位沿原硬件语义，SPIR-V沿原位宽掩码。按位组合与移位在原KnownBits中共同交接；原条件编码集合与多层来源继续沿Issues C-021.1.4.4.3.6.2.1.1.3，移位位事实不自动成为整个字段的精确编码集合。受原窄位移缺陷影响的CPU计算映像须重新生成，公共格式与ABI不因此改版。

数值输入合同使用`requires range(value,lower,upper,...)`：每对端点表示同类型数值闭区间，区间取并集，多条前提分别满足，重叠／相邻区间沿原ScalarRanges合并；`range(value)`表示空集合。value可为普通B按值参数及记录字段／向量分量、普通CPU共同元素路径或整域`data[*].field`；整域沿原普通read／inout字段选择，不假定原子并发输入的共同值。端点使用现有数值字面量、可见模块const及其字段／纯一元二元运算，复用原Builder完整编译期求值，与所选分量标量类型精确匹配；不执行调用或载荷访问，窄整数及i32／i64等类型可由接口中的类型化const给出。浮点闭区间采用数值次序，正负零都属于含零区间，NaN不能作为端点且不属于任何浮点区间，倒置端点拒绝。空dataset全称满足前提，不产生元素、目标非空或访问许可。

普通调用在原Selection检查已有值集合是所声明集合的子集；整数继续消费原Proof范围与KnownBits，浮点将本次值的完整区间集合及原分支路径共同交给既有Condition::covers／Clause::overlaps，逐实际备选核对所需范围的补集，不以共有包络抹去区间间的空隙。原有序浮点比较（==、<、<=、>、>=）为真时，两个已求值操作数都不是NaN，这份事实只由该比较的原上下界交出；比较为假仍保留NaN的可能。原Predicate按F32／F64的实际有序位区间解释比较是否可能成立，包含无穷、正负零及允许次正规输入归零，不把否定比较替换成有序比较、不生成运行保护。旧值快照和改写值继续按各自真实身份解释；字段、向量及无实现源码的对象／归档消费同一合同。callee在同一参数／共同元素模型假定该前提；完整写入及实际相交改写继续沿原事实生产和失效规则，不把输入声明当作输出保证。整域直接生成原ElementFact::Range，由原Program／ExecutionPlan和DomainBindings.require消费已有元数据；普通Vec字节、into_datasets后可改写存储或某批合法样本都不取得输入担保，不增加逐元素扫描、运行检查、资源、拥有表或第二合同。端点在原对象合同中保存类型化常量，无提供者h／b的独立obj／lib不回读实现求值；对象格式保存Range声明编码，旧对象沿原格式边界拒绝并重编，执行映像格式与原资源C ABI保持。外部C／cfn不因本项取得数值range前提。声明不自动生产合法编码或确定资源格式；真实编码生产、通用消费者核对及剩余数值能力分别见Issues C-021.1.4.4.3.6.2／.4.2／.4.3，正式引擎合同与SDK采用由真实消费者接入。

同一编码的允许区间集合可供原整数表达式的关联推导消费。已有两个f32上界在乘法处需要共同基值时，原Builder沿仍存活的Proof、参数环境和原Clause共同事实取得相同已求值来源；原IntegerQuery按原ValueIdentity／Computed／Operand工作栈遍历，每个共享表达式只入序一次。对源集合的每个已有闭区间，两侧使用同一片段，沿原arithmetic_ranges及Scalar的实际位宽、位运算和回绕解释原整数运算，再与已有数值界相交；全部片段都成立才交出严格小于关系，不枚举编码值、lane或运行载荷。

该关系在原FloatingBound合并处把非负仿射上界中的x按x≤d−1改写为原d上界，继续消费原转换、除法及四则运算误差和向外舍入；无符号整数到f32的Cast在非负加法处可形成针对自身来源的上界。实际B算法和机器指令不因推导插入检查或改变运算顺序。数值范围不证明不同元素或已改写快照相等，缺少同源事实时不取得这项新关联；相同类型、字段名或dataset归属不能替代值身份。查询中间材料随本次分析释放，不复制IR、另建表达式图、持久关系／范围缓存或运行拥有者。原Proof及公开数值合同交接已推出的范围，无提供者h／b的obj／lib沿原对象合同消费；对象与执行映像格式与资源ABI保持。真实编码的合法集合与格式由原提供者声明，编译推导不能从某批样本签发它们。

普通tile读取与整数tile_reduce_sum共享这一来源数值查询；求和逐分量消费整个允许输入集合的数值包络与当前有效长度的已知范围，在数学整数中形成总和界，再由原整数模区间算法解释真实位宽及有符号回绕。不能按互不相交输入片段分别假定所有元素取同一片段，也不把某一元素的身份、路径或借用传给总和。结果只在原Proof.ranges中交接，现有模块合同及原索引消费者直接使用；u32逐值在0至1且有效长度至多W时取得0至W；当同一已发布值的真实非零位置还给出数量上界K时，整数求和进一步与非零逐值贡献的总和界相交，不把普通输入的有界数值当作逐lane坐标。F32／F64 tile_reduce_sum复用同一来源与Proof.ranges，将有限输入集合的包络及有效长度上界N交给原浮点区间所有者。原finite与集合输入共用finite_ranges及次正规数解释；最多N−1次非单位元加法的舍入因子(1+ε)^(N−1)按指数位以原有向端点乘法计算；ε为F32的2^-23或F64的2^-52，最小正规数与最大有限值同样消费实际Scalar，覆盖实际线性／树形顺序及允许的次正规清零，不能直接用理想实数的输入界乘长度替代。单元素保留单位元范围，非负／非正输入保留真实符号；结果向外舍入并包含两种零的位模式。只有能建立所有中间加法有限时才交出有限结果区间，不能由最终相消推定中间不溢出；未知、NaN／无穷或无法建立中间有限性的输入保持未知，不限制合法程序或增加运行保护。其他归约／exclusive扫描及尚无共同输入事实的协作参数继续按原Issues对应缺口推进。原IR、CPU／GPU归约顺序、指令、同步及Frame资源所有者保持；不增加运行检查、事实表、发布缓存、格式标签或ABI。分支、重叠及未知写继续沿原模型合流／撤回，prefix、稀疏选择和不同长度的部分覆盖不冒充全体保证；部分整数原子store／exchange、compare_exchange的desired及min／max候选，沿原写效果与未写元素可能保留的旧共同值求并；已知范围或相同逻辑目标可交接后继CPU／GPU，未知写或未闭合的其他RMW撤回。该摘要不作为atomic_load或RMW返回值的稳定读事实，不表示全体命中或完整初始化；原内存序、实际字节、完成和资源拥有者保持。

普通B同一dataset在两条可继续分支都完成相应字段写入后，合流后的元素读取按原分支选择消费已有初始化事实；不同CPU／GPU执行形成的初始化scope分别保留。嵌套分支沿同一选择逐层解释，不能把一侧写入当作另一侧已写。分配时合法的零值保持合法初始化；零值非法变体和原write借用要求的实际未写位置仍不能读取。

普通B的非循环元素赋值在实际存储、初始化及原Data.elements更新后，将本次受影响的真实Frame资源交回原字段完成入口，只发布同一模型已成立的共同数值和必要长度快照。只改一个元素时继续消费原共同模型，不把局部值推广为全体；未写兄弟字段保持原语义。原for循环在正常出口批量交接，赋值入口不在迭代体逐次全域发布；此处自动交接的实际范围不代替其他循环内部／出口的完成能力。数值发布不恢复被计数字段改写撤回的Prefix，失败右值不进入实际store及写完成。后继CPU／GPU范围沿原资源关系消费，不在消费入口补写、不扫描载荷，不增加ABI字段、Frame或状态表。

可信外部字段沿原生SDK的unsafe DomainBindings::produced_element和unsafe Frame::complete_field交接。前者按值消费ElementRange并并入同一原domains；后者从原Ty取得Scalar，与blink_complete_field共同接收ElementInterval数组、u32严格上界数组及原始固定位known_mask／known_value两个u64，两数组仅同步借用。实际字段值必须满足数值区间、固定位及全部严格上界的交集；固定位仅用于整数，value只含mask内该Scalar有效位，未知位不产生保证。提供者须担保实际字节、完整初始化、字段布局、真实完成／原执行依赖及寿命；声明不读取载荷、不完成写或等待，普通安全输入没有自动字段保证。写、可写地址、替换、取出裸数据和释放沿原失效。Stella原函数体及无源obj／lib的独立CPU／GPU路径已完成原生核对，记录归FixedIssues原.4.3.1；实际引擎跨轮采用仍见Issues C-021.1.4.5.2.4.3.2。

整域函数可声明`requires index(data[*].slot,table)`，其中data以普通read／inout读取，所选字段为u32标量或向量分量，table为实际dataset参数。记录字段与swizzle按原类型选择；前提不隐含两个集合逐项配对，也不允许从原子并发输入假设共同值。该输入条件与same_length分别检查，例如：

```b
struct Cell { u32 slot; u32 extra; };
void seed(write dataset<Cell> data) on compute {
    parallel(cell in data) { cell.slot=index()%3; cell.extra=77; }
}
void gather(read dataset<Cell> data,read dataset<u32> table,
            write dataset<u32> output) on compute
    requires same_length(data,output)
    requires index(data[*].slot,table) {
    parallel(cell in data) { output[index()]=table[cell.slot]; }
}
```

动态目标也可直接参与原生产代码，例如：

```b
void seed_dynamic(write dataset<Cell> data,read dataset<u32> table) on compute
    requires same_length(data,table) {
    parallel(cell in data) { cell.slot=(index()+1)%table.length; cell.extra=77; }
}
```

每个实际迭代具有非空的同长目标，源为空时没有求余执行。完整字段的逻辑Data目标与纯数值区间沿同一Proof及模块合同交接；输入要求和输出保证分别使用ElementFact的Range／Index谓词。真实CPU完成时，Index保证按该次实际目标数量成为原ElementRange，与同一实际字段的其他保证求交，运行拥有组只保留数值集合。当前u32目标容量零对应空集合，不能借此制造非空元素或授予部分域完整保证。原目标之后可改写内容、离开作用域或由新目标代替，已有数字的数值保证不依赖它继续存活；下一消费者仍须满足实际容量。当前资源自身的index()也能作为这样的动态生产来源。

独立模块的逻辑目标先按真实参数重绑定；普通B调用把同一结果映射到实际资源根，并消费当时目标长度的已知上界及数值快照。旧目标退出后不保留资源借用或伪造元素值身份；新目标同容量／更大时可继续证明，较小目标仍需真实索引范围，不能靠同形记录或旧资源名通过。

完整原生整域输出的字段区间随原Program／ExecutionPlan交给DomainBindings的同一关系集合。原生CPU生产seed后，后续gather沿同一绑定组消费实际字段保证及table实际容量，允许新的同容量或更大目标；较小目标、未知字节、错误字段／布局和丢弃绑定关系的输入不因此获得前提。源为空时全称前提成立，不制造目标非空。写相交字段、替换存储、可写暴露和into_datasets撤回旧事实，未写兄弟字段保持；join／rebind／partition及冻结只读存储按原身份交接。核对只读元数据，不扫描元素、不复制载荷、不在每个索引处增加运行检查。

这类有输入前提的独立入口属于Compilation.templates／CpuTemplate；无前提入口仍属programs／CpuProgram，不能把条件模板交给裸Vec入口。原B调用继续在编译点证明字段关系。共同前提既可用于当前游标，也可用于其他并行域内的data[index()].slot等实际读取；字段改写后旧前提撤回，未写兄弟字段保持，不能把一个元素的值身份当成全部元素相同。编译器分段分别保存各段实际输入条件和完成保证，不能用整函数最终结果提前许可前段。输出区间只来自实际完整写出，普通参数访问许可不是生产结果。ElementRange使用原Scalar的有序位编码和区间集合：无符号与bool按实际位宽，有符号数以符号位翻转形成次序，浮点正号值翻转符号位、负号值在位宽内取反；这些端点不是未编码的原存储字节。字段类型、逻辑参数、实际绑定和字节偏移各自交接，不保存SSA或条件图。

普通B的dataset<T>(N,F)在原真实初始化循环全部完成后，交接共同字段区间及已证明的自身长度上界。F为静态普通B函数；原游标小于本次数量的关系同时保留真实长度数值快照，后续同容量／更大目标按原index前提消费，不把后来同槽数量当成旧上界。完成描述是原模块常量，独立obj／lib沿原静态包装交接，不读取提供者实现源码；不生成空程序、注册目录或第二事实表。静态空构造不调用F、不制造元素事实，动态零长度的上界对应空区间；工厂失败沿原已初始化前缀结束并退出，未进入完成交接。原字段写即时撤回相交关系，未写兄弟字段和两个独立CPU消费者继续复用原关系。

resize在原存储调整真实成功后，把当前结果共同模型已有的数值区间交给原Frame关系集合；take_last的剩余前缀经同一成功接点交接。增长／缩小与空长度按实际返回数量消费，真实失败不进入完成入口，两个独立CPU消费者、兄弟字段保留和相交写撤回继续原规则；源码缺席的独立obj／标准lib也沿原模块静态完成常量消费。该能力不把旧存储的逻辑Index重命名到新版本，不假称未建立的动态界已经交接。

CPU原生整域纯区间、动态目标关系、普通B工厂及resize已有数值完成和实际保留／新增模型已实现。同长度／缩小不把未采用fill的数值混入保留前缀；动态增长尚未确定时，原结果模型保存当前长度条件下的共同保证，调用前或调用后已经证明没有增长的路径可消费原范围。原Frame的同长度／缩小保留未写纯Element数值，增长撤回旧完整保证，Prefix容量／计数字段关系仍失效；实际分配失败不改变旧长度与事实。fill表达式继续完整求值、副作用和失败，不因模型未采用它的数值而省略调用。

普通CPU完整遍历及GPU完成／传输已闭合，记录见FixedIssues原.5.2.2.3.3及.5.2.3；显式可信外部SDK／Frame入口已经实现；Stella原消费者仍见Issues C-021.1.4.5.2.4.3。GPU独立执行沿原DomainBindings交接同一字段合同：成功提交后的字段数值随原GpuDataset.last_use／Execution及真实依赖使用，后续GPU提交沿原队列接续，CPU读取沿原wait／download。纯区间和本次实际目标数量形成的Index保存在同一ElementRange；传输、冻结共享与绑定重排移动原关系，真实写、可写暴露、替换及释放撤回受影响字段。普通B显式target(gpu,...)及Frame字段投影使用同一合同，正常返回才交回完成；前提拒绝保持未开始的原状态，元数据查询不证明GPU已经完成。条件候选的共同索引域只保留各方独立成立的事实；可空旧前缀的全称保证不能成为实际新增值的存在见证。真实求值结果可以用同一路径上实际参数／局部数值及本次迭代索引的界推出相应索引域，函数返回值与字面值遵守同一规则；原Upper数值界在路径事实中保留相应Below顺序含义。resize实际成功后的完成点同时交接共同纯区间和已证明动态数值上界。数值取本次新长度、原真实参数／局部值、已有旧长度快照及已求值的统一填充值；旧资源版本身份不换成新版本，完成后只保留数字，目标资源之后变化或释放不会改变该数字保证。非严格上界和距界差值都按实际数值集合解释，u32最大端点不制造额外严格界，空集合不制造非空见证。静态模块和源码SDK消费同一字段完成常量，经同一原C入口交给唯一运行关系集合；不增加逐元素扫描、检查或资源保活。尚未形成共同Proof的读前推导精度、GPU和外部载荷仍按原Issues各自范围推进，不通过禁止合法动态输入处理。普通CPU循环结束后，原循环写集合把仍成立的共同字段范围交给同一Frame完成入口；原明确Frame、严格拥有后代范围、本地槽和真正未知存储共同选择潜在写的Frame绑定，CPU位置可来自明确cpu或原Inputs::uses中相同位置在当前路径已成立的Use::At要求。查询不增加或复制要求，不改变位置；没有已成立CPU事实的参数和GPU输入不当作CPU。闭合无写引用按原合同及返回来源形成只读地址，不引入额外完成。只处理原拥有者中可用的CPU存储，实际字段写仍即时撤回旧事实。break只表示控制流退出，不证明完整覆盖；已知旧值与新值共同满足的范围可以交接。return与错误退出不执行后续完成。完整普通数据游标从零覆盖且无本层seek时，原每条正常回边已有的共同数值现在交给同一Data.elements：静态区间和仍成立的u32数值上界共同保留，循环内局部数值身份不逃逸；空域仍按全称语义解释。当前游标的共同末值还须合入原IR中非当前下标写的最终可能值，不能用当轮末写抹去后来各轮对先前元素的修改；动态步长／偏移与普通引用调用按真实存储位置及闭合写效果解释，查询不增加IR、寄存器、存储表或运行保护。显式下标从零遍历到入口同长边界时，实际写右值的数值沿原Clause及原IR存储交到自然出口的同一共同模型；每条正常继续边都须覆盖该字段，其他下标的重叠写也计入跨轮最终可能值。原内存效果解释调用和别名写，未知效果不补造数值；不会生成逐元素检查、另一个数值表或额外执行。普通write／inout dataset形参的共同数值沿原ReturnExit／written交回调用者Data.elements，普通数值元素与拥有元素使用同一模型链；源码缺席的对象／归档及转发调用保留该输出。字段列按实际投影更新，未写兄弟字段保持；共同区间、非零和索引不等式不表示不同元素值相等；完整遍历在原出口绑定后仍有真实共同输入来源时，同一来源随数值模型保存，普通调用代入其实际数值与算术关系，迭代内局部身份不逃逸。U64借用地址与U32 Frame根在原模型中各保留真实宽度，公开dataset布局和机器参数保持。嵌套CPU循环对仍存活外层游标字段的纯覆盖，沿原条件IR、entry、真实继续边与自然出口交接；零次分支按入口事实求值，循环头读取的新值和回边值保持各自时点。完整遍历消费路径约束后的原Proof，原存储／干扰查询递归嵌套Loop，其他下标的真实写仍影响全体结果。私有字段完成调用只更新Frame元数据，不作为载荷写。条件通过地址写兄弟字段的数值精度仍见原Issues对应叶项。普通引用写在实际完整循环结束后，沿原LoopWrites及已成立的CPU驻留要求交给同一Frame；部分路径不制造全体完成。普通dataset调用正常返回后，已有written共同保证由真实调用者沿同一完成C入口交给原Frame；字段列的实际前缀偏移与字段相对偏移共同解释，Host借用保留动态offset，U64原始地址转借不取得Frame。静态模块包装与源码SDK交接同一字段描述、原已求值数值和实际偏移，公共C接口及对象与执行映像格式保持；只读效果不生成重复完成，失败不进入正常完成。静态Data.elements与运行Frame接收仍分别表达；共同输入可为按值或read临时算术实参：完整写入count+3后，可由容量count+4或(count+3)+1的后续独立消费者使用。原完成入口直接消费共同模型中已求值的真实u32寄存器来源，逻辑元素槽不充当机器值，存储不重读。循环头读前事实仍见C-021.1.4.5.4.2。普通B引用的声明权限与Frame可写暴露分别交接：原函数合同已闭合为无实际写，且返回地址及保留原数值存储关系的地址位模式均无可写来源外泄时，引用沿原地址入口只形成读区，不撤回已有字段保证。判定直接查询原Contract.view_access与Returned.proof的views／addresses，包括聚合字段及原返回拥有树中的dataset元素载荷；不建立另一份效果、来源或运行状态。声明Access、原子权限、初始化／借用规则及机器Reference ABI不变。真实写、可写地址或位模式返回、尚未闭合及外部未知调用保留原可写暴露和Frame撤回；普通写后的真实完成继续沿原完成入口，不为只读调用追加完成。静态模型与独立原生输入收到的实际完成分别判断。静态Data.elements与运行Frame已接收分别表达，不能用工厂单项声称其他生产者已经完成。执行映像格式交接Range／Index必要执行合同及标准C字段完成入口；这些是Release必须保持的资源语义，不属于可剥离调试材料，旧资源映像须重编。

## 有界并行接纳与输出前缀

`clear_prefix(output, count.value);` 是 计算函数 顶层阶段，把独立单 u32 Count 写为 0，并建立空的 prefix(output,count.value) 关系。它不清空输出物理存储、不分配另一套队列对象；容量与数据仍由原 dataset 持有。输出必须为普通 write/inout，计数必须为普通 write/inout、形状 1、单 u32，二者不能别名。普通计数值或宿主文件内容仍不能自行构造前缀能力。

在 parallel 中，`append(output, count.value, record)` 尝试接纳一条完整记录，返回 bool。它必须取得已有的输出／Count 前缀关系；输出为普通 write/inout，计数明确为 inout，二者都不能与当前并行域或彼此别名。同一阶段可以多次向同一对输出／计数接纳，也可以操作多个独立输出，但每份输出与计数必须一对一配对，包括嵌套调用、分支与循环中出现的操作。权限／形状错误报 B221，配对冲突报 B222；缺少原有前缀沿用 B191，参数数目报 B220，记录／返回类型沿用 B124。

`.h` 声明对应的数据与相同 计算函数 签名，`.b` 可写：

```c
struct Work { u32 id; bool selected; bool accepted; };
struct Entry { u32 id; };
struct Count { u32 value; };
void build(inout dataset<Work> input,
              inout dataset<Entry> output,
              inout dataset<Count, 1> count) on compute
    ensures prefix(output, count.value) {
    clear_prefix(output, count.value);
    parallel (item in input) {
        item.accepted = item.selected && append(output, count.value, Entry(item.id));
    }
    parallel (entry in prefix(output, count.value)) {
        entry.id += 1;
    }
}
```

计数小于容量时，原语在同一次接纳中取得唯一位置并写完完整记录的所有实际字段，然后返回 true。容量耗尽则返回 false，不增加计数、不写输出、不令计数回绕；这是该操作的容量语义。调用者自行决定如何使用接纳状态，框架不会隐式扩容、重新提交或丢弃错误。参数按普通值规则先求值，即使容量已满，record 表达式的计算也已发生；原语不向源码暴露裸票号或尚未初始化的可写预留区。记录的值字段沿用普通赋值与 ABI，填充保持输出原内容；未接纳的容量尾部也保持原值，不承诺复制来源的填充。

同一阶段的接纳次序不确定；CPU 与 GPU 可以选择不同的成功调用者和输出顺序。每次成功调用必须对应唯一完整记录，失败调用不能占位，实际 Count 等于接纳后的有效数量。它不是稳定 compact，需要输入顺序时仍使用 compact／排序。忽略返回值不会删除接纳效果；短路与三元式只执行实际选中的调用。完整记录构造可以来自普通值函数。普通函数可借用 read／atomic dataset；普通CPU Invocation函数另可转借当前元素的inout dataset权限，但 append 所需的独占输出与关联 Count 仍由 计算函数 的阶段和并行域拥有。

Count 的修改先预留位置，不表示同阶段的载荷已经发布。源码不能把这份 inout Count 或输出别名成原子／只读资源同时窥读，后续消费者通过 计算函数 阶段依赖和 prefix 接口取得已写好的数据。前端使其他共用旧 Count 的关系失效，只保留本阶段证明的实际输出关系；运行时复用既有数据所有权、ensures、DomainBindings 与提交生命周期。条件生产者可 requires/ensures 同一前缀，跨提交继续追加，GPU 不必中途读回 Count。

封闭 Append IR 保存实际输出、Count、完整字段与接纳结果，独占预留和写入不可分割。它参与原有读写依赖、跨元素访问、分支保留、活跃性和原生寄存器分配，不用另一套表达式解释器、宿主回调或队列管理器。CPU 直接用 x64 的容量比较、LOCK CMPXCHG 和记录存储；CAS 成功才写入原位置，失败比较返回的新计数进入同一原子算法。

B 发射 GPU 接纳的活动 subgroup ballot、计数和 rank。选举出的 invocation 读取容量，循环头判断余量，沿与显式原子操作共用的计数读取和 CAS 预留 `min(活动数量, 剩余容量)` 个位置。B 发射范围广播、接纳结果及写入控制，各成功 lane 沿原载荷发射写入自己的完整记录。没有按 32／64 固定 lane 编号，也不为每条记录争用一个全局 CAS。源分歧与不同循环迭代中的活动集合必须准确，因此该 shader 显式使用 SPV_KHR_maximal_reconvergence／MaximallyReconvergesKHR；Device::for_program 按真实程序需求查询并启用 VK_KHR_shader_maximal_reconvergence，普通计算设备不会暗中升级或换算法。缺少所需能力直接报错。该汇合要求来自 [Khronos 最大汇合语义](https://github.khronos.org/SPIRV-Registry/extensions/KHR/SPV_KHR_maximal_reconvergence.html)及其[并行接纳示例](https://docs.vulkan.org/features/latest/features/proposals/VK_KHR_shader_maximal_reconvergence.html)，不是根据本机一次运行推测的保证。

当前是完整记录的有界接纳，不是任意大小预留类型、并发出队／回收、同阶段生产消费、稳定输出、动态设备任务图或自动背压。一般 queue 容器、批量可写 reservation、64 位计数及设备自主调度仍需实现。

## 原生加速结构资源

`read acceleration scene` 是 GPU 计算函数 或 `on gpu` 值函数的不可变资源参数，不是整数句柄，也不占 dataset 长度槽。它只能由执行框架绑定同设备上的加速结构；不能声明为 write／inout，也不能进入 CPU／compute 通用目标。子程序和函数组合保持实际资源身份及只读能力。非法权限或目标在编译期报 B182，查询／借用实参类型错误报 B183，内置类型名称不能被自定义记录覆盖（B184）。

```c
RayHit hit = trace_closest(scene, origin, 0.0, direction, 100000.0);
if (hit.exists) {
    f32 distance = hit.distance;
    float2 barycentric = hit.barycentric;
    u32 instance = hit.instance;
    u32 primitive = hit.primitive;
    float3 geometric_normal = cross(hit.vertex1-hit.vertex0, hit.vertex2-hit.vertex0);
}
bool blocked = trace_any(scene, origin, 0.0001, direction, 100000.0);
```

`RayHit` 的 instance 是构建输入中的实例序号，primitive 是该 BLAS 单一三角形 geometry 内的原三角形序号。exists=false 时 distance 为给定 maximum，其余数值字段为 0；它们不代表一次有效命中。光线原点、方向和区间仍须满足 Vulkan 的数值要求，编译器不能证明动态浮点条件。

vertex0／vertex1／vertex2 是该命中三角形的加速结构空间（无构建前置变换时即模型空间）位置，不是世界坐标。它们与原 primitive 顺序对应；本实现的 BLAS 输入无额外构建变换。实例变换后的外向法线应以逆转置变换原三角形法线，不能直接将负行列式实例的世界边叉积当成外向法线。法线计算属于 B 消费者，不由后端替换成渲染器专用结果。

RayHit 当前 ABI 为176字节、16字节对齐，原字段偏移0／4／8／16／20保持，三顶点偏移32／48／64；object_to_world与world_to_object两个float3x4依次位于80／128。旧的24／80字节完整 RayHit 记录不再是当前 ABI；编译布局、原生调用方和 BLDATA 必须使用当前程序定义，不增加旧记录转换。只保存原基础字段的用户记录仍可保持24字节。exists=false 时被消费的顶点与矩阵分量均为0；不以未命中值进行几何运算。

优化后的 IR 按真实顶点分量使用发现位置读取需求，SPIR-V 只在对应最近命中分支内发出 OpRayQueryGetIntersectionTriangleVertexPositionsKHR。Device::for_program／with_presentation 仅对这样的程序查询并启用 rayTracingPositionFetch、VK_KHR_ray_tracing_position_fetch，BLAS 构建设置 ALLOW_DATA_ACCESS；普通最近命中和 trace_any 不因此申请该扩展。缺少实际能力明确报错，不保留顶点副本或另换求交路径。顶点由 AS 及其原完成点持有，构建输入仍在构建真正完成后释放；读取不延长已无消费者的原上传存储。接口依据 [Khronos 位置读取定义](https://docs.vulkan.org/refpages/latest/refpages/source/VK_KHR_ray_tracing_position_fetch.html)与 [SPIR-V 指令](https://github.khronos.org/SPIRV-Registry/extensions/KHR/SPV_KHR_ray_tracing_position_fetch.html)，本项目直接实现。

object_to_world与world_to_object来自本次实际命中的TLAS实例，分别把物体空间与世界空间互换；包含平移，按B的float3x4行字段使用。逆矩阵由实际AS交付，不在宿主维护第二份矩阵或在着色时再次求逆。外向法线可用transpose(float3x3(hit.world_to_object.r0.xyz,hit.world_to_object.r1.xyz,hit.world_to_object.r2.xyz))变换原物体法线。

优化后的IR分别识别顶点、正向矩阵和逆向矩阵的真实消费；只读取矩阵不申请位置扩展，不消费某矩阵就不发射该矩阵读取。B 发射器直接使用[SPV_KHR_ray_query](https://github.khronos.org/SPIRV-Registry/extensions/KHR/SPV_KHR_ray_query.html)的OpRayQueryGetIntersectionObjectToWorldKHR／WorldToObjectKHR（6031／6032）；标准结果为四列float3，逐分量转换为B的三行float4。两项仍属于原RayQueryKHR能力，无额外设备扩展。只在真实命中分支读取，未命中分支形成零值Phi；不把CPU矩阵表作为GPU查询结果。

两个查询都可在最后提供 `bool cull_back_faces`。省略或传 `false` 时按原行为接受两面，传 `true` 时 Vulkan 在候选确认前剔除背面三角形。同一个加速结构可接收不同射线的选择。查询处理不透明三角形，使用固定 0xff 可见掩码，`trace_closest` 返回最近的合规交点，`trace_any` 在首次合规命中时结束。面向按 [Vulkan 三角形面向剔除语义](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-face-culling) 判定。

前端生成封闭的 RayQuery IR，B 发射器写出 SPIR-V 1.4 与 SPV_KHR_ray_query 的初始化、遍历和结果合流。同一着色器按实际使用共享顶点数组和矩阵的类型声明。CPU 后端明确拒绝硬件查询操作，不转成另一种求交算法。GPU 值函数可借用命名的 `read acceleration` 并调用其他 GPU 函数；内联时绑定调用方的真实资源下标，不复制加速结构、不增加描述符，也不允许把资源当普通值返回。仅借用 acceleration 与普通值的辅助函数同样使用该资源调用规则，不要求同时声明 dataset／texture。查询最终仍在调用 计算函数 的并行域执行；不可变数据 view 可以经 CPU／compute／GPU 函数组合。

执行框架 `Bindings` 同时接收 datasets 与 accelerations；提交对象在实际完成前持有全部资源。顶层结构的唯一所有者持有去重的 BLAS 引用；只读 acceleration 可以共享同一所有者。由 `Device::for_program` 从程序资源需求选择设备能力，创建加速结构的低层 unsafe 接口明确要求调用者提供有效几何及有限、可逆的实例变换。材质和数值算法正确性不由资源类型证明。

原生框架的 `bottom_level_updatable`／`top_level_updatable`／`top_level_device_updatable` 明确以 ALLOW_UPDATE 构建；原静态入口保持。`refit_bottom_level(&source,positions,indices)`、`refit_top_level(&source,instances)`、`refit_top_level_device(&source,data)` 使用 Vulkan UPDATE 生成新的独立结构，同样返回 `BottomLevel`／`Acceleration`，不改变 B 的 `read acceleration` 类型、绑定顺序或查询 ABI。原结构及其克隆继续表示原快照；派生结果按自己的真实构建／查询完成点持有资源。

refit 来源必须属于同一逻辑设备且在构建时声明可更新，primitive／instance 数量及三角形 maxVertex 保持。索引内容／顺序与活动状态保持、有限顶点／有限可逆矩阵是相应 unsafe 输入前提；形状、原生更新能力及设备归属由资源入口报告，不能将其解释为任意拓扑修改的许可。GPU 原生实例输入直接消费原 64 字节记录页及地址数组，也接受冻结数据的 `binding()`；同形的 CPU／GPU 实例入口可交接。只读资源没有原地写入权限。

执行在更新真正完成前持有 source 的实际结构引用、输入与 scratch，新 TLAS 持有本次实际 BLAS 集合；is_complete／wait 观察本次更新本身，后续查询仍由各自提交负责。原生输入与 scratch 可以在本次更新完成后退出，旧查询需要的旧结构继续有效。`Acceleration::allocated_bytes` 仍只计结果 AS 与其直接持有的去重 BLAS 存储，不计 pending 操作临时持有的旧 AS、scratch／输入，不作为更新峰值、整卡驻留或共享资源的全局去重统计。

GPU 几何的原生入口为 `bottom_level_device(vertices,triangles)`、`bottom_level_device_updatable(vertices,triangles)` 与 `refit_bottom_level_device(&source,vertices,triangles)`，消费实际 GpuDataset，也接受冻结来源的 `binding()`。顶点 offset 0／4／8 为三个 f32，沿实际 stride 取样，位置后可有其他字段；三角形为紧密三个 u32、stride 12，例如 `struct Triangle {array<u32,3> indices;};`，`uint3` 的 16 字节 stride 不是该原生索引布局。消费完整记录域，不根据 GPU 有效前缀隐式截取；初始化、有限位置及索引范围是 unsafe 前提。

源数据页沿原传输完成点复制到具备 AS 输入用途的连续原生存储，不下载几何、不拆分三角形或改变 primitive 编号。源存储保持到复制完成，输入保持到 AS 构建／更新完成，共享与退出复用原实际所有者。CPU／GPU 几何在原数量、maxVertex、索引内容／顺序与活动状态相同的前提下可交叉 refit，顶点 stride 使用本次布局；原结构仍是原快照。

当前 refit 是显式的新版本 AS 更新，覆盖 CPU／GPU 几何 BLAS 与 CPU／GPU 实例 TLAS；原地更新、通用拓扑修改、压缩及完整流送尚未实现，未新增 B 内的资源构造语法或通用版本／热更新能力。原生依据为 [更新约束](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdBuildAccelerationStructuresKHR.html)，具体 API、压力失败及原生范围见 [设备实例](DeviceInstances.md) 与 Issues。

光线查询不要求窗口：`Device::for_program` 不启用 surface／swapchain。需要原生窗口呈现的程序明确使用 `Device::with_presentation`，计算资源和执行语义保持相同。`Device::swapchain(instance,window,width,height)` 接收实际输出尺寸；呈现消费同设备、等像素数和 4 字节 stride 的 BGRA8 dataset，并逐实际物理页复制，页边界可位于像素行中间。真实表面尺寸和设备能力属于原平台入口，框架不写入某个应用的最低分辨率。

## 原生只读二维与三维纹理
`texture2d`／`texture3d` 可用一个格式实参声明实际资源要求，例如 `read texture2d<rgba8_unorm> noise`。支持当前真实texel编码 `rgba32_float`、`rgba8_unorm`、`rgba8_srgb`、`r8_unorm`、`rg16_unorm`、`rgb10a2_unorm`、`r11g11b10_float`；名称区分大小写，它们是纹理格式标记，不是普通数值／记录类型。相应数组写为 `read array<texture2d<rgba8_unorm>,N>`。未写格式保持原任意格式接口，指定格式调用只接受具有同一明确格式的原资源参数；可向未指定格式的形参借用，不能从未知格式反向取得明确要求。接口与实现必须声明同一格式，维度、数组形状及只读寿命保持原义。原CPU／GPU绑定入口在执行前消费实际资源格式元数据，数量／维度／设备归属仍在原入口检查；不检查每个texel，也不改变机器采样算法。独立普通CPU调用在公开接口中保留要求，原生外部调用者必须交付符合要求的原纹理描述。该格式声明须与实际坐标／LOD的原数值事实共同消费，不能仅凭UNORM名称假定采样结果。现有静态分析在坐标每分量已证位于[-1,1]、LOD已证位于[-2^32,2^32]时，对rgba8_unorm／r8_unorm／rg16_unorm／rgb10a2_unorm交接[0,U]，U是(1+10^-4)^25按原浮点区间规则向外形成的F32上界；rgba8_srgb含额外曲线，继续原保守[0,2]。坐标与LOD的这个范围属于当前共同分析覆盖，不是合法采样域限制；其他合法输入仍可采样，范围保持未知。RGBA32F、R11G11B10浮点和未指定格式不产生归一范围，非有限或未知输入也不产生本范围。原Proof继续承载四则、floor、整数转换、源函数及obj／lib输出；不生成运行检查、钳制或替代采样。Stella外部离散编码及完整散射仍见C-021.1.4.4.3／.4。

`texture2d`／`texture3d` 是不可变资源参数，分别匹配二维／三维坐标，供 CPU／compute／GPU 计算入口、源内展开与普通CPU原生函数只读借用；它们不属于普通值类型，不能放进普通记录、内联 array、dataset、view、tile、统一值或函数返回值。接口分别写 `read texture2d`／`read texture3d`，组合时传递实际资源身份，不复制像素、不暴露地址，也不能将普通值冒充纹理。多个只读参数可以引用同一资源；固定资源数组见下一节。Windows x64普通CPU函数在源码、独立obj及标准lib中交接同一只读纹理借用，支持单张、固定资源数组、选中元素转传与递归。公开声明继续保留维度、形状与索引前提；机器输入和寿命见[普通纹理参数ABI](NativeInterop.md#普通b纹理只读参数的机器交接)。当前编译模块的普通调用为CPU调用，调用方须具有合法CPU执行目标；显式on cpu整域调用和其无源obj／lib可沿原Frame同步消费该借用，包含分段域、数组与选中项；GPU整域输入仍见C-003.2.3.1.2.2，GPU独立普通调用仍见C-021.3.1，完整compute目标及B拥有纹理构造按各自未完成能力记录。

```c
// example/texture.h
module example.texture;
struct Sample { float2 uv; f32 lod; float4 colour; };
void shade(inout dataset<Sample> pixels, read texture2d albedo) on compute;
```

```c
// example/texture.b
module example.texture;
void shade(inout dataset<Sample> pixels, read texture2d albedo) on compute {
    parallel (pixel in pixels) {
        pixel.colour = texture_sample(albedo, pixel.uv, pixel.lod);
    }
}
```

`texture_sample(resource, coordinate, f32 lod)` 返回 float4；二维资源要求 float2，三维资源要求 float3。通道格式及精度见下表。坐标归一化，U／V／W 越界由资源逐轴选择 Clamp／Repeat；各轴 texel 中心为 `(index+0.5)/extent`。二维层内双线性、三维层内三线性过滤，随后线性混合相邻 mip；显式 LOD 限于 `[0,mip_count-1]`。mip 由资产明确提供，各轴尺寸依次为前层除以 2 并至少为 1；允许合法的部分链，不隐式生成 mip。除 sRGB 的 RGB 通道外不进行显示传递转换。

资源维度、参数数量、坐标／LOD 类型、只读权限与不可存储性在编译期检查，分别使用 B239 或既有类型诊断 B124。当前跨后端数值约定以有限坐标／LOD 为前提，编译器尚无一般有限值证明，不增加逐次采样的运行时修正。采样的目标精度和允许差异见下节；返回 float4 不表示原生过滤的所有中间量都按 CPU f32 求值。

源采样进入同一 TextureSample IR，TextureBinding 保存实际静态维度，结果与坐标／LOD 参与原活跃性、死值删除、静态函数内联和 tile 私有值交接。GPU 后端的 B 发射器按该维度直接写出图像与采样类型、描述符读取及显式 LOD 指令。资源数组选择发射对应的 NonUniform 装饰，过滤由原 Vulkan sampled image／sampler 执行。CPU 后端沿同一降低器读取 mip 元数据，二维取四点、三维取八点，逐轴插值后混合 mip，使用 64 位 texel 地址并直接生成 x64 指令，没有逐 texel 的宿主回调。CPU采样降低在实际生成指令时保留原采样来源与cpu_texture_sample生成职责；普通函数的同次NativeCode操作范围据此交接，观察材料不改变执行代码或ABI。含采样的 map 当前使用标量范围代码，采样通道裁剪、重复采样复用与更宽 SIMD 仍需后端工作。

### 采样数值与目标精度

采样先解释原通道，再在每个被选 mip 内过滤，最后混合相邻 mip；输出是 float4，不再量化回资产通道位宽。七种格式的通道解释如下，均不预乘 alpha。格式解码和缺失通道遵循 [Vulkan 图像读取](https://docs.vulkan.org/spec/latest/chapters/images.html#images-reads)，sRGB 传递函数见 [Khronos Data Format 的 sRGB EOTF](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html)。

| 资产格式 | 进入过滤的通道值 | 精度来源 |
|---|---|---|
| RGBA32F | 原四路 f32，不作显示传递转换 | 原浮点值与过滤算术；不引入 UNORM 编码间隔 |
| RGBA8 UNORM | 四路分别为原 u8 除以 255 | 八位输入编码与归一转换，再叠加过滤算术 |
| RGBA8 sRGB | RGB 先按 UNORM 解释再转线性，alpha 仍除以 255 | 八位输入、sRGB 解码及过滤算术分别贡献误差 |
| R8 UNORM | `(r/255,0,0,1)` | 红通道八位输入及归一转换，缺失通道按固定值补齐 |
| RG16 UNORM | `(r/65535,g/65535,0,1)` | 两路十六位输入及归一转换，不按八位格式精度处理 |
| RGB10A2 UNORM | RGB 分别除以 1023，alpha 除以 3 | RGB 十位与 alpha 两位分别解释，不共用同一编码间隔 |
| R11G11B10 FLOAT | 无符号浮点RGB，R／G各6位尾数、B为5位尾数，共同5位指数；alpha为1 | 原11／11／10浮点及过滤算术，保留HDR范围，不按UNORM归一 |

CPU 由现有 f32 标量指令完成坐标运算、通道转换及插值，sRGB 幂运算沿原数学函数语义；不为模拟某块 GPU 而把轴分数或结果额外定点量化。GPU 使用同一原始格式的 Vulkan image／view，minFilter、magFilter 与 mipmapMode 均为 LINEAR，归一坐标、零 LOD bias、逐轴 Clamp／Repeat，关闭各向异性与深度比较；原图像、sampler 和最后消费者的所有权保持。

GPU 的坐标与权重按 [Vulkan Sampling](https://docs.vulkan.org/spec/latest/chapters/textures.html) 定义：每个被选 mip 分别用其真实尺寸缩放归一坐标，再按 texel 中心定位相邻采样点。轴内分数保留的精度由所选物理设备的 subTexelPrecisionBits 表达，显式 LOD 及相邻 mip 的混合分数由 mipmapPrecisionBits 表达；两者是不同环节，见 [VkPhysicalDeviceLimits](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceLimits.html)。这些属性不表示所有设备采用同一个位数，也不表示把每个最终多轴／跨 mip 权重积直接舍入到同一网格就一定符合规范。轴与 LOD 分数恰可表示时，后续乘加及格式解码仍有各自精度要求。

格式转换、过滤中间算术与 float4 输出还受 [Vulkan Numeric Representation and Computation](https://docs.vulkan.org/spec/latest/chapters/fundamentals.html#fundamentals-numerics) 的要求约束。该规范对着色器外浮点运算规定约十万分之一量级的单次运算精度，并明确源 texel 格式可以隐含限制表示／精度；这不是每次完整采样统一的绝对误差，也不能替代轴／mip 量化及具体格式规则。着色器内 f32 指令的精度和浮点控制不自动成为固定功能过滤器的中间算术保证；原始位的无损资产／传输亦不保证过滤保持 NaN 载荷、负零或次正规位型。

范围分析与精度约定分别处理：理想线性过滤是非负、权重和为1的归一texel组合，实际CPU lerp及GPU过滤仍有舍入。普通UNORM的共同外界消费Vulkan着色器外最低约十万分之一精度，只取更宽的epsilon=10^-4，沿当前三轴加mip至多4维16项计算；一条贡献的权重、texel、累加和转换至多25个舍入因素，形成[0,U]。CPU四级lerp及仿射解码包含于同一更宽界，sRGB额外曲线仍用[0,2]。原数值所有者向上计算端点，并向F32外舍入，正负零都保留；不钳制结果、不增加设备条件，也不把这个保守界当作允许实际实现降低精度。相同原输入只改变编译期事实，具体返回精度、采样指令及资源仍遵守原合同。

同一算法、格式和输入不承诺 CPU／GPU 或不同设备逐位一致；允许差异仅来自上述实际格式与目标规范，不是任意误差许可。完整结果误差还与所选相邻 texel 的值、局部变化、mip 内容和实际运算有关，不能由某一个编码间隔或精度位数推得通用常数。当前没有跨目标严格一致的采样模式；已知原生过滤偏差仍待解决，不能仅以“硬件精度”认定其合法。真实材质效果由消费者核对，不以本数值合同承诺天然材质视觉等价。

### 原生纹理接口与寿命

原生资源 API 是 `Texture2D::new(Vec<TextureMip>)`／`Texture3D::new(Vec<TextureMip3D>)`、各自的 read／write，以及 `Device::upload_texture/download_texture`／`upload_texture3d/download_texture3d`。CPU 的 `TextureInput::{D2,D3}` 持有相应原资源，GPU 的 `vulkan::TextureInput::{D2,D3}` 持有 `Texture<2>`／`Texture<3>`；两类 Bindings 的 textures 都是相应 TextureInput 列表，可用 `.into()` 移交或共享原资源，不复制像素或 GPU 图像。CPU 交给 `CpuProgram::run_owned`／`CpuTemplate::run`，GPU 普通程序与条件模板使用同一 submit／submit_after／submit_join。每项与原 TextureBinding 核对实际维度，GPU 继续核对设备归属。数组展开及资源列表顺序仍按各类参数在 `.h` 中的声明顺序；Vulkan set 0 依次放 dataset、acceleration、texture、执行器私有辅助项。CPU 集合范围摘要使用原私有槽，不把纹理描述误当摘要。

CPU 调用期间持有实际像素及描述；Device::upload_texture 返回完成原生提交的共享图像，返回时不等待设备复制。主机 mip 已进入本次 staging，源 Texture2D 可以退出；上传命令与 staging 由原 Transfer Execution 持有。Texture::is_complete()／wait() 只观察该图像的生产复制，不代表后续采样已经结束；采样提交自动收集真实图像生产依赖，调用者可以直接交付尚在复制的 Texture。GPU Commands 持有 image／view／sampler 到原生采样消费者结束，沿既有 Execution 和完成点释放。调用者保留 Texture 克隆即可供后续提交复用，不从已丢弃句柄或裸地址重建资源。Count 的域关系仍随实际数据所有权移动，纹理不引入宿主计数镜像、独立调度服务或资源注册表。绑定数量／设备归属、资产文件边界及物理格式能力属于既有资源边界。

Device::copy_texture(sources, dimensions, format, address, levels) 沿同一Device直接把GPU数据复制为二维Texture，不经CPU像素或另建合并缓冲。sources为可变GpuDataset切片；每级TextureCopyLevel给出source下标、该源中的offset记录起点及row_stride记录行步长。每条源记录是一份已按format编码的texel，实际stride必须等于format.texel_bytes；同一源可承载多级，不同mip也可来自不同源。每级尺寸由首级二维尺寸逐次减半并至少为1，允许部分链；输入明确已有像素，不隐式生成、量化、拼图集或重排引擎数据。创建处核对真实尺寸／mip、行与源范围、设备归属及原格式能力；按真实物理页和行拆分vkCmdCopyBufferToImage区域，不设置经验容量阈值。原生产写入到Transfer读、Transfer读到后继Compute／Transfer写及目标采样布局都有真实访问交接。只有提交成功后把同一Execution交给实际源last_use；源退出沿原Drop／discard持有至复制完成，后继采样沿原Texture／Submission持有图像至最后消费者。复制读取不把源producer伪记为写。Texture的完成接口观察本次图像生产，不代表以后采样完成；调用者可在同一队列继续重写源，图像保持已复制的独立存储。

Device::download_texture(&texture) 显式等待真实 image→buffer 复制和采样布局恢复，返回独立 CPU Texture2D 的完整 mip；源纹理可以继续共享和采样，返回值在源 GPU 引用退出后仍有效。操作从实际图像读出，不缓存原 CPU 像素、不经采样重建颜色、不翻转行。传入另一逻辑 Device 的图像明确报设备归属错误；需要交接时，调用者先用源设备读回，再把该 CPU 数据交给目标设备上传。独立逻辑设备及本机离散／集成两块物理 GPU 的双向显式交接已有原生记录，不表示直接设备间通信或隐式迁移已经完成。Texture::is_complete／wait 仍只观察原图像生产，采样结果仍沿实际提交观察。

二维独立资产 `.bltex` 写入 **BLTEX v2**：8 字节 `BLTEX\0\0\0`，小端 u32 version=2、format、mip_count；format 0／1／2／3／4／5／6 分别为 RGBA32F／RGBA8 UNORM／RGBA8 sRGB／R8 UNORM／RG16 UNORM／RGB10A2 UNORM／R11G11B10 FLOAT。每层为小端 u32 width、height，再跟按行紧密排列的原始 texel，分别每 texel 16／4／4／1／4／4／4 字节，RG16 为小端 R、G 两个 u16；RGB10A2 为小端 u32，R／G／B／A 位于 0..9／10..19／20..29／30..31，R11G11B10同样为小端u32，R／G／B位于0..10／11..21／22..31，5位指数／偏置15，尾数为6／6／5位；零、次正规、正常、Inf／NaN按真实浮点编码解释，不作UNORM转换。文件层间无填充。现有 v1 资产仍明确解释为原 RGBA32F 布局，未知版本／格式直接报错。行方向由资产生产者确定，读取不翻转像素。CPU 资源按真实 u32 维度、可表示的总字节数、完整负载与 mip 布局接收，二维／三维共用原字节大小计算；原生寻址使用 64 位整数，不以 f32 坐标精度设置资源尺寸上限。坐标运算仍遵循实际 f32 精度。GPU 受实际 image2D／image3D 尺寸、内存和所选格式的线性过滤／传输能力限制。文件读取仍为显式同步操作；GPU 上传显式构造主机 staging 并提交异步设备工作，不等于完整异步流送或虚拟纹理。

当前计算入口、源内展开及普通CPU原生调用支持只读二维／三维采样；显式on cpu整域入口与无源obj／lib沿原Frame同步借用实际纹理，GPU整域输入仍见C-003.2.3.1.2.2，GPU独立普通调用仍见C-021.3.1，未解决的 NVIDIA 三维过滤偏差仍见 C-003.2.4.1.2，不宣称全部目标已通过。固定纹理资源数组见下一节；压缩与整数采样、可写图像、二维数组图像、通用 bindless、隐式导数、各向异性、一般资源泛型及运行时自动 mip 构建尚未实现。规范依据 [SPIR-V 标准](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html) 和 [Vulkan 图像格式](https://docs.vulkan.org/refpages/latest/refpages/source/VkFormat.html)。

BLEngine 的 `Programs/material/textured.h/.b` 使用同一资源语义实现批量 GGX，CPU／GPU 均实际执行。引擎全图入口现已消费 `.blmat` 材质资产、几何分组与原始 UV，具体格式和职责见 引擎内部材质资产资料（不公开）；天然纹理、完整光传输和白玉视觉目标仍未完成。

### 三维体资产与原生传输（Rust SDK）

`Texture3D::new(Vec<TextureMip3D>)`持有原不可变体mip，`TextureMip3D`的width／height／depth与`TextureTexels`共同定义每层真实布局。每层x连续、再y、再z；三轴逐级整除二并至少保留一个texel，可交付合法部分mip链，不自动生成。整链同一实际格式，支持上述六种通道编码，负零、次正规、NaN及所有原始位由字节交接保持；没有解码后的float镜像。构造要求非空、正维度及可表示且完全匹配的载荷／mip布局，不把 f32 坐标精度当成体资产尺寸限制。

`dimensions()`返回基础`[u32;3]`，`mip_count()`与`mip(index)`只读借用原层级；克隆共享同一Arc。`with_address(u,v,w)`消费该资源视图并设置三轴Clamp／Repeat，默认三轴Clamp。`Texture3D::read/write`为显式同步资产I/O，读回新建Clamp视图；寻址配置归使用方，不写入资产。

BLTEX v3体资产的8字节magic仍为`BLTEX\0\0\0`，接小端u32的version=3、dimension=3、format、mip_count。每层为小端u32 width／height／depth，再接x、y、z顺序紧密排列的原texel位；通道格式编号和字节布局与上文相同，层间没有平台填充。三维读取入口只接受该三维编码，二维入口仍解释其原v1／v2；未知版本、维度、格式、截断、非完整布局或未声明尾部数据直接返回原错误，不将三维解释为二维图集。文件未保存设备图像、采样器或运行期资源身份。

`Device::upload_texture3d(&Texture3D)`返回`vulkan::Texture<3>`，默认`vulkan::Texture`仍为`Texture<2>`。`Texture<3>::dimensions/address`分别返回三分量数组；format、mip_count、allocated_bytes及is_complete／wait继续观察同一原图像／上传完成点，上传完成不代表后续采样完成。二维和三维上传使用同一个原Storage／Image／Execution主体，imageType／viewType分别为实际2D／3D，三维arrayLayers=1；尺寸由extent.depth表达，逐mip深度随宽高一起减半。图像创建处读取[VkImageCreateInfo的格式／维度能力](https://docs.vulkan.org/refpages/latest/refpages/source/VkImageCreateInfo.html)及实际过滤／双向传输位，尺寸、mip或采样数不可用即沿原Result返回，不静默替换图像或格式。

CPU mip交接至原staging并成功提交后可退出，staging及原图像保持到真实消费者结束；克隆不产生第二份image。`Device::download_texture3d(&Texture<3>)`从实际图像完整复制并同步等待，恢复原采样布局，返回独立CPU `Texture3D`且保留格式、每级宽高深与三轴寻址。复制按[VkBufferImageCopy](https://docs.vulkan.org/refpages/latest/refpages/source/VkBufferImageCopy.html)的真实三维extent紧密寻址，R8等格式的逐级staging起点保留4字节对齐，文件无此填充。另一逻辑Device的图像仍明确拒绝。二维／三维的传输和元数据接口在Rust类型上区分，原 `vulkan::Bindings::textures` 通过 TextureInput 接收混合维度列表，沿同一上传依赖和最后消费者持有图像。

体资产、原生传输、计算入口的 texture3d／float3 采样、CPU三线性与SPIR-V三维图像绑定沿上述同一资源交接。普通CPU纹理函数与obj／lib消费原只读描述借用；CPU整域函数及无源obj／lib也沿原Frame同步借用这些原描述；GPU整域输入和GPU独立普通调用仍待实现，NVIDIA原生过滤偏差仍待解决。BLEngine实际Filmic LUT、效果与性能由引擎消费者交付，不由普通函数、传输或单设备成功推断全部能力完成。

### 固定纹理资源数组与逐元素选择

`read array<texture2d,N>`／`read array<texture3d,N>` 在计算入口、普通函数及 cooperative 参数中声明固定长度、维度一致的只读纹理集合，N 是正 u32 编译期形状，可用模块 const 或有限／开放 const 参数。调用可用实参资源数组的固定长度推导 N。它是资源接口，不具有普通内联 array 的值布局，不能嵌套或放入记录、dataset、view、tile、局部普通值、返回值；普通数值数组的原规则保持。集合中的每张纹理可以有不同宽高和 mip 链；该接口表达独立图像的描述符集合。

`maps.length` 是静态 u32 常量，`maps[index]` 选择资源，可直接采样或传给匹配维度的只读纹理形参；整组可借用给相同维度与长度的资源数组形参。下标必须为 u32，常量范围、数值范围、length 条件和函数 `requires index(index,maps)` 沿用原证明。越界或无证明报 B137，错误形状／资源权限报 B239；源程序没有隐式下标修正，运行时直接执行已证明的访问。带写效果的下标只求值一次，CPU 后续 mip 与 texel 读取也使用同一索引结果。

外部资产边界已经建立有效编号时，可在采样或只读资源借用位置写 `unsafe_read(maps,index)`，例如 `texture_sample(unsafe_read(maps,material.texture),uv,lod)`。它选择一张资源，仍要求资源数组与 u32 下标；调用方负责 `index < maps.length`、实际绑定与资源寿命，编译器不声称已证明该数据关系，也不生成钳制、替代采样或逐像素校验。普通 `maps[index]` 的证明要求和 B137 保持。所选资源可传给匹配维度的只读纹理形参，不能存入普通值、记录或逃逸；选择仍使用同一 Texture IR，索引只求值一次。

BLEngine 的正式消费者在读取 `.blmat` 时解析并核对 surface 的纹理编号，再把完整只读材质表和图像集合交给提交；计算程序只消费这份已交付数据。这里的外部编号关系由资产所有者承担，不增加另一份运行时校验状态。

下面是循环纹理序列：frame 是持续增长的帧号，按集合长度取模表达纹理序列的播放周期。

```c
// example/texture_sequence.h
module example.texture_sequence;
const u32 FrameCount = 3;
struct FrameSample { float2 uv; f32 lod; u32 frame; float4 colour; };
float4 sample_frame(read array<texture2d,FrameCount> frames, u32 frame, float2 uv, f32 lod) on compute requires index(frame,frames);
void animate(inout dataset<FrameSample> samples, read array<texture2d,FrameCount> frames) on compute;
```

```c
// example/texture_sequence.b
module example.texture_sequence;
float4 sample_frame(read array<texture2d,FrameCount> frames, u32 frame, float2 uv, f32 lod) on compute requires index(frame,frames) {
    return texture_sample(frames[frame], uv, lod);
}
void animate(inout dataset<FrameSample> samples, read array<texture2d,FrameCount> frames) on compute {
    parallel (sample in samples) {
        sample.colour = sample_frame(frames, sample.frame % frames.length, sample.uv, sample.lod);
    }
}
```

源资源引用保存原 binding 与可选元素寄存器；动态元素属于 IR 输入，参与原重映射、死值删除、寄存器活跃性与跨 tile 阶段保存。资源选择也参与值的 uniform 判定，不能因 UV／LOD 是常量就把逐元素选纹理的结果当成统一值。函数静态内联保持实际资源及元素，不复制像素或把采样移成宿主逐元素调用。

GPU 一个资源数组占一个 descriptor binding，其 descriptorCount 为 N；不同 invocation 可以选不同元素。同一程序按原TextureBinding的实际数组元素合计描述符数，设备创建直接消费候选已查询的maxPerStageDescriptorSamplers、maxPerStageDescriptorSampledImages、maxDescriptorSetSamplers和maxDescriptorSetSampledImages四项容量；独立程序各自形成布局，多程序选择取各程序总数的最大值。实际装入继续消费已确定设备的容量，并在该调用内求一次总数供纹理及组合资源检查使用。B 发射器写出数组声明及实际选择的 AccessChain，为索引、指针和采样图像写入 `NonUniform` 装饰，按 [SPIR-V descriptor indexing](https://github.khronos.org/SPIRV-Registry/extensions/EXT/SPV_EXT_descriptor_indexing.html) 编码。`Device::for_program/with_presentation` 为资源数组明确查询并启用 [shaderSampledImageArrayNonUniformIndexing](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceDescriptorIndexingFeatures.html)，不支持或未声明设备能力时明确报错；不改成逐材质宿主提交。CPU 按绑定表中的实际资源指针寻址，整体数量与地址保留原生宽度，仍由原范围线程执行。

`Program::textures()` 返回按参数顺序排列的 TextureBinding，count=None 为单张纹理、Some(N) 为资源数组；`texture_count()` 是实际元素总数，`texture_offset(binding)` 是该参数在扁平资源列表中的起点。`cpu::Bindings.textures`／`vulkan::Bindings.textures` 仍为同一实际所有者列表：按参数顺序，再按每个数组的元素顺序展开。例如数组 3 张、单张、数组 2 张，共三个纹理 binding、六个资源元素；执行器辅助 binding 在三个资源 binding 之后。物理能力按元素总数判断，不误用参数个数。

CLI 同样在所有 dataset 文件之后，按上述展开顺序提供每张 `.bltex`。现有格式不变，数组不向资产写入平台句柄或地址。所有 N 个槽必须绑定实际纹理；共享同一图像时显式克隆只读所有者即可。Commands 持有整个本次实际使用的资源列表至原生完成，没有可变描述符更新、占位纹理、全局资源注册表或新的提交寿命。

`material.textured::indexed_ggx<N>(maps,material,point)` 已将此接口用于真实 B 材质求值，使用开放 `const N:u32`，要求调用前证明 `index(material,maps)`；内部调用原 textured_ggx。N 可从实际纹理数组的静态长度推导，也可显式给出；一般资源类型泛型仍未实现。集合长度目前参与编译，运行时可变长度、部分绑定、描述符在途更新、纹理句柄进入普通记录与通用 bindless／虚拟纹理仍未实现。

## 不可变 CPU／GPU 数据视图

`view<T>` 是带元素类型与长度的不可变数据引用，可嵌入结构体。例如 `struct Instance { view<Triangle> triangles; view<Normal> normals; float4 tint; };`。`.length` 为 u32；由该引用的长度条件或有界游标证明后可用普通索引读取，外部索引契约仍使用 `unsafe_read(instance.triangles, primitive)`。源程序不能提取地址、把 view 转成数值或写回引用元数据。

本节的宿主持久共享view图可作为read或inout整域参数；CPU／compute／GPU遵守同一共享图规则。普通CPU应用拥有树的整元素移动按下文[CPU拥有元素的dataset](#cpu-owned-dataset-elements)交接。inout按原权限更新不含view的普通兄弟字段及其数组分量，原view载荷与来源保持；实际含view字段或整记录不能作为原始载荷写入或普通write／inout位置借出，compact／排序／append也不能复制view载荷到持久共享输出。不含view的字段可按既有权限转借给普通函数。write完整根不能构造view图，atomic根及整域按值view仍不开放；只读view元素不获得写权限。本地普通view值及聚合仍可按既有来源规则组合、赋值和返回，不把持久图的写入限制扩大为局部值限制。根和共享子资源继续由原Dataset／GpuStorage.views及最后真实CPU／GPU消费者持有，字段更新不改图、不复制资源、不新增完成或所有权表；view机器布局与资源C ABI保持现行定义。无写副作用的普通值函数可以按值接收／返回view或含view记录，按原目标规则组合；不相容目标仍报B161。伪造数值地址或写view载荷报B185；可变view及完整语言资源泛型尚未实现。

宿主用 `Dataset::for_view` 获取元素类型和布局。CPU 用 `Dataset::freeze()?` 交还可跨宿主线程共享的 `blcompute::ReadDataset`，以 `Dataset::with_views(Vec<ViewBinding>)` 绑定父记录；GPU 上传后用 `GpuDataset::freeze()` 交还 Vulkan 的 `ReadDataset`，通过 `Device::upload_with_views` 绑定已有设备数据。两端共用字段、逐记录数量、元素类型和唯一性处理，GPU 另外检查实际设备归属；不以地址字节代替真实资源。父数据持有按所有者身份去重的子引用；CPU 原生范围完成前保持根数据，GPU 提交继续持有根数据到 Fence 完成，子资源随最后一个真实引用释放。嵌套图通过相同不可变引用闭合，不增加宿主每元素回调。

view 沿上文的 24 字节、对齐 8 的机器表示交接实际地址、长度及步长。CPU 使用冻结存储的连续字缓冲和 64 位元素寻址。GPU 使用现有物理页数组，B 发射器按实际记录步幅选页并计算记录内地址。IR 的 ViewLoad 使用同一个 address 字段。SPIR-V 对物理页数组的地址加载声明 Aligned 8，B发射的载荷访存按实际物理单元对齐并按低字在前的顺序拆合值，遵守 [Vulkan SPIR-V 环境规则](https://docs.vulkan.org/spec/latest/appendices/spirvenv.html)。view地址与所有权由执行框架生产，view不公开其隐藏地址；独立CPU address值不绕过该view的资源与寿命。

### 主机资源的可变访问与传输

先填写普通字段，再冻结子数据并绑定父记录。`ReadDataset` 只提供不可变借用；冻结会转移唯一可写存储，源缓冲不复制，子引用由标准 Arc 持有，可供多个宿主任务共享。`with_views` 根据真实来源编码隐藏地址、长度与实际元素步长，并持有去重所有者；相同资源可以被多个字段、记录和父对象引用。

`Dataset::words_mut()` 表示重新取得原始可写 payload，同时放弃该数据已有的视图引用所有权。旧的地址字节不能继续作为有效绑定；需要使用仍由调用者持有的实际只读来源显式重新 `with_views`。非空含 view 数据若只有原始字节、文件内容、脱离所有者的设备地址副本或已丢弃绑定，在 CPU 执行／冻结／上传的既有资源边界明确拒绝，不从地址重建引用。空父数据没有记录和子字段，无需虚构子资源。

上传／下载是显式物理传输。`Device::upload` 复制根数据及其闭合不可变引用图，目标父记录由实际 GPU 子所有者生成地址与长度；`Device::download` 对完整图读回，并生成持有真实主机子资源的 Dataset，可直接交给 CPU 程序或冻结共享。调用方无需逐层重写目标绑定，已有 GPU 子资源仍可用 `upload_with_views` 显式组合。复制包含地址的裸记录不等于传输资源图。

父数据保存去重的实际来源及每个记录字段对应的来源槽；物理地址只是从该图生成的执行表示。每次传输用局部所有者身份映射共享目标：相同来源只复制一次，内容相同但所有者不同的来源不会合并。`upload_domains`／`download_domains` 在本次所有根之间共用这份局部映射，并保留实际存储的有效域关系；调用结束后不保留迁移缓存、地址注册表或后台任务。分别调用两次 upload／download 不承诺跨调用共享。

GPU 页按完整记录传输，目标地址直接编码到每页现有暂存映射，不额外复制整个父 payload。CPU 地址指向连续缓冲，GPU 地址仍指向页表；普通字段、填充和 view在偏移20至23的尾部填充保持。BLDATA 当前只保存原始记录，不序列化资源图；文件内地址不能作为绑定，也不能生产 Count 所有权。CPU／GPU 的内存不是统一地址空间，没有隐式执行设备选择、按需迁移、自动多设备迁移；缓冲区异步传输见“原生缓冲区与AS异步交接”；可写 view 仍未实现。CPU 直线及统一控制流的视图读取可进入下述四元素降低，逐元素分歧继续由原生标量控制流及既有范围线程池执行，不使用解释器。只读视图不把硬件光线查询或 subgroup 变成 CPU 功能。

```rust
let gpu_data = device.upload_domains(cpu_result)?;
let pending = gpu_program.submit(gpu_data, &[])?;
let cpu_data = device.download_domains(pending.into_bindings())?;
// cpu_data 同时持有读回的根、不可变子图及原实际有效域关系。
```

### 引用值的范围与遍历

```c
let normals = instance.normals;
let i = item.normal_index;
let valid = i < normals.length;
if (valid) {
    item.normal = normals[i].normal;
}

for (normal in instance.normals) {
    let i = ordinal(normal);
    // normal 是该引用内的只读元素游标。
}
```

view 与普通 dataset 共用范围条件、bool 命题、函数内联、多个资源的联合范围及有界 for 语义；未知普通访问报 B137。可以读取整记录、字段、向量或嵌套 view，也可在各自条件成立的 `?:` 分支中读取。`for (i in 0..normals.length)` 取得同一引用的索引能力；`for (element in view表达式)` 允许结构体字段、函数返回值和条件选择式，表达式及其长度在进入循环前只求值一次。游标支持既有 ordinal、break、continue 和 seek。引用变量在循环体内改变时，当前游标仍指向进入循环时捕获的引用，不能把游标范围转给新引用。

范围身份属于实际引用值，包含隐藏页表地址两部分与长度三个分量的来源，不能仅凭变量名、存储槽或两个资源碰巧等长判为同一个引用。编译器沿不可变别名、可变局部、嵌套记录字段、按值参数和返回值保留既有 SSA 来源；替换整个引用或某个记录字段时更新对应来源，其他未变字段保留自己的来源。分支合流与循环仍撤回可能被改写的旧知识。例如先保存 `let old = normals`，再把 normals 改成另一个资源，旧条件只能约束 old；重新比较新 normals 的长度后才可索引它。

compute中从dataset记录直接重复读取同一未改写view字段时，同一真实数据绑定、实际下标及字段分量可沿原IR装载和写效果消费已经成立的长度事实；例如if(rows[index()].values.length>0)后的rows[index()].values[0]不要求另存局部view。不同元素／字段、引用替换、实际或未知相交写不能继承旧读取关系，独立原子装载也不成为稳定内容。一般无游标view元素重读的等价关系仍未实现；已经保存的不可变记录或局部记录字段继续保留分量来源，精确长度相等也能按既有规则向索引传递多个资源范围。这些元数据只存在于前端，不增加设备字、运行时版本号、范围检查或 GPU 提交。普通索引与 `unsafe_read` 共用既有 ViewLoad，含 view 的资源仍由原父数据和提交消费者持有到完成。

view 的元素及元数据始终不可写，长度证明不能取得散写权限。资源布局与绑定有效性由既有执行框架边界负责；加速结构命中、文件内几何索引等外部数据关系尚不会自动变成语言范围证明，现有渲染程序继续显式承担这些外部索引契约。

### 只读资源函数

```c
float3 surface_normal(Instance instance, u32 primitive, float2 barycentric) on compute;
```

函数可以用 `unsafe_read(instance.triangles, primitive)` 和 `instance.normals` 完成只读几何计算；实际实现见 `Programs/render/trace.b`。它也可以返回 `view<T>` 或包含 view 的记录，再由调用方读取。这里复制的是不可变引用值，不复制底层数据，不改变资源所有权；所有引用仍来自已绑定的 read 根数据，宿主持有整条依赖到最后一个提交消费者完成。没有设备分配、裸地址构造或可写资源逃逸。

只读资源函数可声明为 `on compute`，或明确为 `on cpu`／`on gpu`。函数签名、参数类型与调用方目标由相同前端检查，静态内联后生成同一 ViewLoad IR；两个后端直接读取各自绑定的物理存储。实际 `surface_normal` 已由渲染器与 CPU／GPU 共同消费。普通函数可以进一步借用 read／inout／atomic dataset，CPU Invocation及on compute／on gpu源码展开共同沿实际字段和当前下标转借；原计算函数继续拥有阶段和并行域。普通函数独立检查按inout声明取得已有初始化与独占借用前提；真实并行调用处仍核对实际Access、字段重叠与当前位置。有效下标证明不授予任意非原子散写能力，read或atomic来源也不能升级成inout；共享读写冲突仍按原规则拒绝。GPU数值运算继续要求设备支持相应真实精度与浮点特性，不隐式改变精度或转CPU执行。外部索引有效性仍是调用者责任，不因提取成函数就变成静态证明。

## 外部索引边界

`unsafe_read(read_dataset, u32_index)` 读取具有外部索引契约的数据。它只能用于不可变 read 数据集，不能用于别名读取正在并行修改的数据。调用者负责索引有效性、设备和资源生命周期；编译器不把它描述为静态范围证明，也不生成自动修正索引的代码。普通 `dataset[index]` 要求域、固定形状或已成立的长度条件证明，未证明的访问仍报 B137。

引擎用此入口读取索引三角形与共享顶点：OBJ／.blgeom 读取已经负责外部资源索引有效性，每项几何通过自己的类型化 view 读取局部三角形与法线索引。引擎内部若破坏该契约，仍可能产生非法 GPU 访问，编译成功不能保证它正确。后续的带来源索引类型尚未实现。

### 长度条件与有界索引

```c
struct Input { u32 value; };
struct Work { u32 source_index; u32 value; };
void gather(read dataset<Input> input, inout dataset<Work> work) on compute {
    parallel (item in work) {
        let i = item.source_index;
        if (i < input.length) {
            item.value = input[i].value;
        }
    }
}
```

编译器从程序自身的条件得知 i 在 input 的范围内，随后生成既有 Load IR。它不另外插入检查、修正索引、限制到最后一项或改用其他数据。这里只说明访问在实际进入的分支内有效；未进入时做什么由源码定义。读取同一数据的 `unsafe_read` 写法在相同源码条件下可生成相同的原生代码。

当前识别命名 u32 值与 dataset／tile length 的严格比较，支持反向比较、否定、if 的真／假分支、while 的成立分支，以及 &&／|| 实际执行右侧的条件。例如 `i < input.length && input[i].keep` 的右侧已具有索引能力；`i >= input.length || input[i].keep` 的右侧同样如此。已经有效的索引也可作为其他索引的上界或相等值。`i <= input.length` 会允许等于长度，不能产生有效索引能力。

`for (i in begin..input.length)` 在循环体内具有 input 索引能力；tile length 与同源的不可变长度别名也适用。范围起点和终点在进入 for 前各求值一次；起点大于等于终点时不进入循环。已经有效的索引作为严格终点时，内部游标也在同一数据域内。改变保存终点的局部变量不会改变已捕获的本轮 for 范围。

索引与长度能力沿用前端 Value／Binding 的 domains 集合，不成为运行时资源。一个实际值可以同时属于多个数据的索引范围，也可同时保存精确长度来源与其他范围事实。不可变 let 复制保留这些事实；可变绑定赋值时用右侧新值的事实替换原集合，不能把新旧值的范围合并。分支各自在入口环境分析，退出恢复外层事实，并清除任一路径可能改写的局部知识。while／for 进入前先清除循环体可能改写的局部知识，while 再从本轮实际条件取得能力；不能把首轮有效的索引沿用到后续轮次。这与优化器的循环携带值处理分别负责类型安全与值转发。

范围证明只解决索引边界。read 数据仍不能写，普通写入仍只允许当前独占元素；已完成初始化的字段可按C-029读取，阶段共享只读见“parallel阶段的共享只读访问”，tile 仍是不可变快照；不能借长度条件获得其他元素的写权限。dataset.length 是绑定容量，筛选后的有效条数继续使用 prefix 及其接口／所有权关系。布尔值可保存范围命题；不可变 view 的范围见前节。下文列出的 u32 单调索引算术已实现；任意算术范围推导、完整关系求解和用户构造索引句柄仍未开放。

### 多数据范围与长度来源

```c
let i = item.source_index;
let valid = i < positions.length && i < normals.length && i < materials.length;
if (valid) {
    item.position = positions[i].value;
    item.normal = normals[i].value;
    item.material = materials[i].value;
}
```

同一索引可同时读取分别存储的数据；后一个范围不会覆盖前一个。各数据长度可以不同，实际进入分支的条件由源码决定。嵌套分支、反向比较、bool 别名、相等索引、按值函数与返回值沿用同一事实集合，没有额外的设备检查或数据布局协议。

`let n = positions.length; if (n == normals.length && n == materials.length)` 使 n 同时具有三份精确长度来源；内部的 `for (i in 0..n)` 可以读取三份数据。相等条件把当前两侧已经持有的事实交给另一侧；严格小于某个精确长度才产生对应索引能力，n 自身仍不能作为 positions[n] 的索引。长度事实与索引事实并存，例如 n 是 positions 的长度且小于 normals 的长度，`for (i in 0..n)` 对两份数据都有效。

动态 `choose ? i : j` 只保留两侧结果共有的范围与长度；常量选择保留实际选中值的集合。逻辑 OR 的替代路径也只保留共同事实，不能把只在一条路径上成立的能力合并到全部路径。常量折叠的比较仍保留成立结果的命题，已知左值的短路求值也不丢失左侧提供的事实。

同一阶段的 tile 输入与发布共享既有逻辑块宽度和实际尾块长度，因此块内范围使用共同的 Tile／TileLength 语义，而不是存储资源编号；实际 TileView／存储引用仍各自独立。由 points.length 证明的块内索引也可读取同阶段同块的 keys。现有宽度约束继续生效，这不授予跨阶段访问、可变共享存储或其他元素写权限。

同一 dataset 的重复 length 与同一逻辑块的 tile length 具有稳定身份，详见下一节。当前路径显式证明 `na == nb && nb == c.length` 后，na／nb 可取得 c 的长度来源；这属于原 Proof 中已证相等快照的传递，不建立独立持久关系图。不同资源仍保有自身身份，只能在真实长度关系成立的路径传递索引范围，不授予写权限。缓存谓词的循环回边、任意算术与外部资源重复装载的等价关系仍见 C-005；尚未证明的用法明确报错，不代表已经完成全部范围推导。

### 资源长度的稳定身份与实际执行域

`dataset.length` 来自当前程序绑定的完整容量；同一实际数据集的多次读取具有同一编译期身份。`if (table.length != 0)` 成立时，之后的 `key % table.length` 可以证明除数非零，`if (table.length >= 8)` 可证明读取 table[7]。不再要求先人为声明一个 n 才能把同一个长度关联起来。read／atomic 借用、多层函数转交、向量／记录字段与 bool 谓词沿用同一 Proof；例如返回 `table.length != 0` 的谓词函数可以让调用方在成立分支内安全读取该 table。

事实仍受原有分支作用域约束：离开 if、进入另一分支或让 OR 的另一条件单独成立，都不能保留未必成立的长度下界；短路与三元式的临时事实也不泄漏。不同 dataset 不能混用身份；相同元素类型或实际长度碰巧相同不产生范围关系。普通数据字段与 atomic_load 的重复读取没有因此变成稳定快照，Count.value 更不是 dataset.length；相关重读等价关系仍未实现。

有界 for 的游标比较现在复用普通比较分析。在循环体内，当前游标严格小于进入循环前捕获的无符号终点，因此这个终点至少为 1。可以直接写：

```c
u32 rotate(read dataset<Cell> table, u32 key) on compute {
    u32 result = 0;
    for (cell in table) {
        result += table[(ordinal(cell) + key) % table.length].value;
    }
    return result;
}
```

同一规则适用于 `for (i in 0..n)` 的未被改写 n、命名的不可变 view，以及 tile 游标；空范围不会执行循环体。起点、终点和 view 仍只捕获一次，body 内改写原终点变量不会改变本次范围，也不能让新值继承旧终点的非零事实。循环体中的事实在离开循环时撤回，不据此断言循环至少执行过一次。

parallel 的某个元素实际执行时，它所属数据集的容量至少为 1，因此本域内 `key % jobs.length` 可以证明非零。prefix 域执行时同样只证明承载它的数据集非空，不把容量当作有效 Count；空有效域没有正文执行。该事实不能交给其他数据集，也不改变只有当前元素可独占写入的权限。

tile length 属于当前逻辑块，其输入、发布、字段借用与 cooperative 参数共享该长度身份。长度下界可由 cooperative 返回的 bool 传回调用方，也可用于同块另一 tile 的普通索引；实际尾块仍按有效长度判断，不把固定逻辑宽度当作尾部容量。没有共享值冻结或隐式屏障。

这些是前端已有类型／范围事实的来源和作用域：资源借用持有长度的 Proof，普通值仍持有自己的值身份，既有环境负责分支保存／恢复。没有独立的事实管理器、GPU 长度缓存、宿主计数镜像、运行期边界检查或新增 IR／设备 ABI。长度读取仍生成原来的 Uniform／TileLength，再由共用优化器消除冗余；并非把跨屏障值强制留成新的私有流。

### 布尔命题与值来源

```c
let i = item.source_index;
let valid = i < input.length;
if (valid && item.keep) {
    item.value = input[i].value;
}
```

bool 现在可以保存比较所得的范围关系，再经别名、`!`、`&&`／`||`、bool 相等／不等、布尔 `?:`、按值函数参数和返回值传递。范围事实属于参与比较的值，不按变量名猜测。例如 OR 成立时，只保留每条可能成立的路径共有的事实；`i < input.length || item.keep` 不能单独证明 i 有效。布尔值转成数字、写入 dataset 或发布到 tile 后读回，或经过 subgroup 运算后，不会自动保留这种来源关系。

```c
u32 i = item.source_index;
let original = i;
let valid = i < input.length;
i = 0xffffffff;
if (valid) {
    item.value = input[original].value;
}
```

旧条件仍可约束保留下来的 original，不能约束已改写的 i；若显式把 original 赋回 i，二者又是同一个实际值。更换了长度变量也不会改写已计算的 bool：当时参与比较的长度若已知来自 input.length，命题仍按这个原值成立。任意新的算术结果不因变量名相近或数值可能相等而继承关系。

普通值继续复用既有 SSA 值编号记录来源，资源长度使用前述稳定身份；index()与当前有界游标的ordinal重复求值保留同一实际索引身份，并消费当前路径已证范围、相等及非零事实，条件中的直接读取、requires合同与整数除法不要求人为绑定快照；局部槽在赋值、分支合流及循环改写处更新或清除知识，不生成运行期版本号。if 真／假两侧、短路右侧与选择表达式各自消费实际条件；作用域结束后只保留真实继续路径的共同范围事实。未被调用改写的局部位置保留首次读取取得的实际值身份；write／inout 调用则由原位置写入更新对应字段事实，不能把旧快照的条件交给新内容。函数内联按实际参数的来源推导返回的 bool，现阶段没有另行声明／导出的通用函数精化类型。

u32／i32 的显式相等关系沿原 Proof 传递：`a==b && b==c && c!=0` 可以证明 a 非零，u32 的 `a==b && b==c && c<input.length` 可以读取 input[a]。嵌套条件、保存的 bool、返回谓词及两种等式顺序使用同一来源关系；OR 只保留每个可能路径共同成立的等价类与数值事实。每个分量仍保留自己的实际来源，额外记录已证相等的快照，合流取共有来源；赋值替换目标分量，不改变旧快照。比较另一个分量、使用不相等路径、跨出关系作用域或改写后沿用旧 bool 都不能取得许可。浮点相等、任意算术恒等式和原子重复装载不因本项变成同一值。

只读dataset的parallel／for游标，以及不可变view／只读tile的for游标，沿实际当前位置保持固定字段、静态数组分量及view元数据的来源。例如 `item.index < item.rows.length` 可供同一元素的 `item.rows[item.index]` 使用；`requests[ordinal(item)]`和`entries[ordinal(item)]`访问同一实际来源与行时也消费这份关系。view／tile行的未知字段身份随游标建立，if／bool／continue及普通函数消费同一内容事实；实际地址和活动借用保持。不同元素、不同字段或已改写的内容不能继承它，for 推进与 seek 重入按本轮实际元素重新建立关系。只读权限、原子访问限制及真实资源所有权保持；dataset记录未改写的同位view重读沿view章节的原IR读取关系交接；其他无游标位置的一般重复装载、一般代数等价、一般循环不变式与跨原子装载等价关系仍未实现；同一已绑定下标的动态数组选择沿固定array章节的原内容／路径规则。这些关系仅用于编译期，不增加运行时缓存或范围检查。 只读tile按实际tile来源、行及字段识别，包含输入捕获和普通发布的tile；捕获后原全局元素被修改不改变已有tile值。cooperative沿调用方实际块范围交接，atomic tile仍须显式原子访问，不能因游标复用内容关系而取得普通读写权限。

普通 if 在两侧都继续时合并两份实际环境；只有一侧继续时只消费该侧。未改变的分量保留原身份，改变的分量只保留各条路径共同的非零／索引域和覆盖两侧的数值区间。共同上界按实际快照交接：一侧的端点等于原长度、另一侧严格小于同一长度，合流后仍能证明端点不大于长度，可作为半开view区间的末端；直接索引仍要求严格小于长度。两侧对同一界的已知距离取共同可保证的距离，不把其他长度来源、未证明分支、改写或回绕当作相同关系。一侧最大值不大于另一侧最小值时，较小侧同样保证后者已有的上界，不把长度相等、最小值或地址关系反向复制。普通值选择与分支赋值复用这一合流。标量 bool 的 false／true 条件分别与实际分支条件组合，例如 `if (i<input.length) { ok=true; } else { ok=false; }` 后，`if(ok)` 可以读取 `input[i]`。某侧可能未经证明就令 ok=true 时，合并不能交付索引关系。循环条件失败与各个 break 的真实出口也保留共有的 bool 条件；这不表示循环回边的缓存谓词关系已经实现。原来的确定初始化、字段别名和控制流退出规则保持，不生成运行时合流状态。

循环入口按实际可能写入的字段／swizzle／固定数组投影撤回事实。例如 `uint2 pair=uint2(1,3); for(i in 0..n){pair.x=i;}` 仍可使用 `99/pair.y`；借给 write／inout 的字段、while 条件中的写调用与通过数据集名字访问当前元素也进入同一写集合。字面量及已捕获的不可变局部常量下标使用对应数组元素，动态下标覆盖所有可能元素，不把可变下标的入口值当作整个循环的固定位置。投影不再次执行下标或调用；真实动态数组写入只撤回所属数组子对象，外层其他字段继续保留事实。已改写分量仍不继承旧身份。字面false&&／true||的短路右侧，以及同一已知布尔关系确定未选中的?:一侧，不进入循环写集合；条件及实际可能执行的分支效果保持；函数不可变性由原写入标记确定；本地bool只有在原完整写范围排除实际和未知本地写后才交接入口Proof，并同时供条件与普通调用实参消费。普通值和布尔事实的分析不要求该值具有CPU连续存储位置，tile游标可按原值规则读取；实际read／write／inout／atomic实参的位置及初始化／借用要求保持。筛选后的写由原投影形成同一完整摘要，装载、数值和资源沿同一关系；其他可变或未知位置不沿用入口事实。动态未知仍消费两侧，声明阶段仍保留两侧类型／访问检查需要的位置存储。确定位置的u32分量沿同一写集合分析常量位移、独立赋值或同一常量的反向关系`x=c-x`，while与有界for共用原数值保留入口，只交接初始值与每条真实继续边共同保证的数值界。本地拥有dataset的长度赋值复用同一独立赋值和控制流合流，当前消费本次resize或dataset构造的确定长度参数；参数表达式只求值一次，字面量、循环中不变的命名u32快照或位置按原规则提取。仅交接数值界，不恢复旧资源域或相等身份，也不把可变长度改成固定count。位移在已知范围内不得回绕；反向关系保留初始范围与一次反向范围的并集，包含实际u32回绕，同一反向与恒等路径可顺序组合和合流。当前不同反向常量或与非零位移的组合不据此生成范围，这属于分析精度边界。独立赋值当前识别字面量、命名不可变u32值或循环中未改写的u32位置，非恒定具名来源只在无seek循环中提取。被覆盖前的未知效果不冒充赋值结果，后续完整独立赋值可以建立自己的界。continue与seek结束当前路径并交接已执行效果，不消费其后的不可达赋值；break按真实出口参与合流。普通循环沿原LoopState保留实际继续边，仍引用入口值的自然出口分量与这些回边交接共有数值。原LoopControl用入口的真实输入重绑定首个条件，分别保留零次与已经执行路径；当前变量被改写不冒充原输入快照。改变的数值身份沿原output_aliases和Clause合流，实际B写入后不继续使用入口常量或非零许可，已取出的SSA快照仍独立。view借用、Position初始化与资源所有权继续由原消费者处理，没有运行时出口标记、第二张图或另一份拥有状态。B调用进入循环体预分析及复合静态分支的剩余写扫描分别按Issues的具体缺口建设。嵌套循环的未知写入仍为未知，一般循环不变式、缓存谓词回边和动态数组内部的具体位置关系仍待实现。

`while (inside(i,input.length))` 或含短路／三元表达式的循环条件可在每轮重新建立范围，前提是 inside 按值返回相应比较结果。首轮留下的旧 bool 不能证明后续改写的 i。即便源码在循环末尾重新计算了缓存 bool，跨循环回边的关系合并目前也未实现，涉及的普通索引仍可能报 B137。把 bool 写进数据集再读出、转成数字再转回，或用任意布尔等价关系与完整关系求解重建证明，同样不能假称已实现。C-005 保留。

这些来源与命题只用于编译期，不进入 IR 数据布局、资产、GPU 缓冲或提交协议；不会增加范围检查、索引修正、数据副本或宿主任务。相同的直接条件与不可变 bool 别名算例已生成完全相同的 x64 和 SPIR-V。

bool 的存储字以 0 表示假、非零表示真。CPU 标量／SIMD／tile 读取和 GPU from-word 解码在取得语言值时规范为 0／1，相等、取反、数值转换和普通类型化复制使用同一逻辑语义；未写的源存储不变。compact／排序搬运完整记录仍保留原始记录字节，不把记录搬运与 bool 值求值混为一件事。

动态 `?:` 也从条件取得各自分支的范围事实，并且只求值选中的一侧，见下节。普通 dataset 未证明访问报 B137，tile 未证明访问报 B201，写权限仍由 B144／B166／B202 拒绝。

### 无符号索引算术与下界

u32 标量现在可以根据实际值的下界继续推导普通索引。`i > 0`／`i != 0` 的成立路径持有 i≥1；`i >= 3`、`3 <= i`、与已知下界值相等或比较也产生相应下界。命题仍沿既有 bool 别名、取反、短路、选择与按值函数传递。AND 合并必然事实，OR 只保留各条成立路径都保证的下界，例如 `i >= 3 || i >= 7` 保证 i≥3，`i > 0 || choose` 不保证非零。

```c
struct Work { u32 key; bool head; };
void mark_runs(read dataset<Work> input,
                  inout dataset<Work> output) on compute {
    parallel (item in output) {
        let i = ordinal(item);
        if (i < input.length) {
            bool first = true;
            if (i > 0) {
                first = input[i].key != input[i - 1].key;
            }
            item.key = input[i].key;
            item.head = first;
        }
    }
}
```

这个程序可对已排序记录标记连续键段的起点，再交给既有 compact 形成分组头队列；没有单独的宿主邻域任务。前驱读取的条件是源算法处理第一个元素的规则，编译器不会另加边界检查或修正索引。

| u32 标量表达式 | 产生索引能力的条件 |
|---|---|
| `i - c` | c 是编译期常量，当前事实证明 i≥c；结果继续属于 i 已有的数据／块／view 索引域 |
| `i / d`、`i % d` | d 是编译期非零常量或已证明非零的动态 u32 标量；结果不会大于 i，保留 i 的索引域。余数还严格小于 d，可取得 d 的实际长度来源对应的索引域 |
| `i >> s` | s 是编译期 0–31；结果不会大于 i，保留 i 的索引域 |
| `i & j` | 结果不会大于任一操作数，可同时持有二者已有的索引域 |

下界在无回绕减法、常量除法和右移后继续换算。动态选择的数值结果只保留两侧共同保证的下界，已知常量选择沿实际选中值；因此 i≥3 时，`j = choose ? i-1 : i-2` 保证 j≥1，后续 `j-1` 仍可安全索引。常量赋给可变局部时捕获其实际值下界，重新赋值、分支合流和循环改写仍由既有来源与失效规则处理；旧 bool 不授予新值下界。

长度本身仍不是索引。若显式捕获 `let n = input.length`，在 n>0 的路径中，`n-1` 属于该输入；`n-0`、`n/1`、`n>>0` 仍等于长度，不能访问该输入的第 n 项。掩码不无条件继承 length 的资源索引身份，但其结果可由独立数值界证明，例如长度已知为 17 时 n&15 必然在范围内。正的常量减法必须不下溢；非零长度作为取余的除数时，余数严格落在该资源内。

上表描述与具体资源绑定的符号索引域，适用于普通 dataset、tile 输入／发布及 CPU／GPU 不可变 view，资源身份与写所有权保持。这些关系按实际 u32 分量生效，不因一般加乘、左移或 OR 就复制操作数的资源身份；数值界可按下一节独立证明结果。算术本身仍按既有 32 位语义执行；证明不足只拒绝其普通索引用途，动态整数除余的非零要求见下节，未证明或不支持的除数仍报 B152。数值元数据只存在前端，不进入 IR、ABI、GPU 缓冲、资产或提交协议。

### 逐分量资源域与批量索引

每个 u32 分量分别记录它属于哪些 dataset／tile／view 的索引域，以及它是否是某个资源的精确长度。原 Value 与 Local 上的整值 domains 已删除，域与数值界、非零事实、值来源都由现有 Proof 持有。向量构造、广播、swizzle、嵌套记录、按值参数／返回值和局部字段投影不会丢失对应分量的域，也不会把一个分量的许可复制给其他分量。

```c
struct Cell { u32 value; };
struct Work { uint4 keys; uint4 result; };
void lookup_pair(read dataset<Cell> left,
                    read dataset<Cell> right,
                    inout dataset<Work> work) on compute {
    parallel (item in work) {
        let sizes = uint2(left.length, right.length);
        if (sizes.x != 0 && sizes.y != 0) {
            let ids = item.keys % uint4(sizes.x, sizes.y, sizes.x, sizes.y);
            item.result = uint4(left[ids.x].value, right[ids.y].value,
                                left[ids.z].value, right[ids.w].value);
        }
    }
}
```

四个 ids 分量在同一普通值中保存，但 x/z 属于 left，y/w 属于 right；对二者长度没有相等要求。`left[ids.y]` 仍无法证明。逐分量除数已证明非零时，除法／取余沿实际操作数计算每个结果的域；常量右移与 AND 同样逐分量适用；无下溢的常量减法也逐分量适用。AND 可以同时保留两侧索引域；长度本身不因此变成索引。

条件比较一个字段时，只精化持有同一值来源的对应分量。打包长度的正下界可证明该资源的常量索引，例如 sizes.x>0 时读取 left[0]；若该分量被替换成普通输入，旧长度事实随之移除。局部分量赋值保留未改写字段，分支选择只保留两侧对应分量共有的域；保存的旧别名继续描述旧值。循环入口按可能写入的投影撤回对应分量，其他分量及其已有条件关系保留；已写分量的一般循环不变式仍未实现。

同一规则覆盖只读 view、tile 输入、tile 发布与 cooperative 借用。实际尾块长度参与取余，不能拿固定逻辑宽度索引较短尾块。资源身份仍取自真实引用；打包、拆包及传参都不会把一个 view 的范围授权给另一个 view。当前函数静态内联，调用处传递实参的编译期事实，不是 `.h` 新增了依赖索引类型或跨 ABI 的证明协议。u32／i32 显式互转保留原位模式的资源域，中间 i32 仍不能直接作为索引；恢复 u32 后才可消费原域，数值来源身份不跨类型复制。

这些值仍使用原来的标量／向量／记录布局，CPU 和 GPU 消费既有 IR；没有运行时范围对象、自动索引检查或额外提交。唯一可写元素仍由 parallel 的真实所有权决定，组内序号和有界数值不能授予任意散写。

### 逐分量 u32 数值区间

u32 值现在同时携带数值下界与上界，向量及记录按实际分量保留。字面量、固定形状、算术、比较、别名、字段、值参数和返回值共用现有 Value／Proof。下标全部可达整数片段的上界都严格小于固定容量或当前作用域保证的最小有效长度时，可使用普通读取；这与已有资源索引域共同服务同一检查，不另建运行期检查。矛盾条件产生的空集合包含于任意有效下标域，包括零长度输入；它不表示下标0可达。源类型、只读权限及并行独占写入检查保持，改写局部值后必须按新值重新证明，不能沿用此前空域。

```c
struct Cell { u32 value; };
struct Work { u32 key; u32 value; };
void lookup(read dataset<Cell,17> table,
               inout dataset<Work> work) on compute {
    parallel (item in work) {
        let i = item.key % 17;
        item.value = table[i].value;
    }
}
```

工作域可大于 17，甚至与表完全不同形；余数是 0–16，不再要求人为补一个 i<table.length 条件或使用 unsafe_read。已证明非零的动态除数也可限定余数上界；掩码、右移、无回绕加减乘和左移继续计算数值界。OR／XOR 使用保守的有效位上界，按位取反及 u32 取负按真实 32 位结果换算，bool→u32 的范围为 0–1。

u32／i32 的同值运算可使用已经保存的同一整数快照，或当前路径明确证明相等的快照关系。x-x 与 x^x 的结果范围为零；已证明 x 非零后，x/x 为一、x%x 为零，x&x 和 x|x 保留该实际值的范围。u32 同值 AND／OR 也保留原索引与精确长度事实，例如 for(i in 0..(table.length|table.length)) 仍按该表的长度遍历。向量与广播逐分量判断，不能仅因两个值具有相同区间就套用此规则。局部或资源字段改写、函数写效果及不同 OR 路径继续按原值身份撤回关系；浮点不采用这些整数范围恒等式，数值事实不授予并行散写。

整数结果确定等于实际操作数时，同时继承该快照的身份、非零和资源域：包括 x+0、0+x、x-0、x*1、1*x、x/1、中性按位运算、有效移位量只含零的左右移，以及已证明两侧同值的 AND／OR。因此 table.length+0 或 table.length*1 仍可作为同一资源的遍历长度，y=x+0 后的 y!=0 也可约束仍持有同一快照的 x。标量广播与向量分别交接实际分量；i32 中不改变位模式的恒等运算可保留潜在域，恢复 u32 后才可用于普通索引。非恒等分量、被改写的新值、浮点运算和 f32／u32 bitcast 不借用这些身份。运算和副作用仍照原顺序执行，不把恒等式当作删除调用的许可；数值相同也不替代 parallel 的真实当前位置。

可能回绕的加减乘／左移按低 32 位投影区间，跨断点时保留可能值的片段；包围结果覆盖完整周期时使用完整整数域，不沿用错误端点。移位量先按有效低五位推导，负数或大于 31 的原始计数不会改变这条规则。u32 右移保留不大于原值的关系；源值已证非零、有效移位量也排除零时，结果严格小于原值，真实非空长度可由此产生安全索引。原始移位量非零不保证有效量非零，空长度与 i32 算术右移不借用该严格关系。整数比较沿同一可达区间片段及实际快照判断真／假结果是否可能：排除无交集的相等、严格端点外的顺序关系，以及同一整数快照不可能的比较结果；i32 继续使用翻转符号位的有序编码，跨回绕片段分别判断，空集合不制造端点。只剩一个结果时，既有 bool 条件交接为已知值，选择／if／短路只按原已知条件规则消费可达结果；比较操作数仍按原顺序求值，字段或函数写入后的新快照不借用旧关系。浮点不套用整数同值或区间判断，NaN 语义保持。比较的真／假路径继续产生原上下界，选择式按可能值合流，短路 OR 路径只保留共同成立的事实。字段赋值只替换对应分量，未改写的值及显式旧别名保留原事实；分支退出与循环改写仍撤回既有作用域知识。矛盾条件形成空集合，不对不存在的端点执行除法。

u32 的正分子除以至少为 2 的除数，商严格小于分子；例如 table.length!=0 且 divisor>=2 时，可直接读取 table[table.length/divisor]。这沿实际长度来源交付索引事实，标量、向量分量、常量和动态除数共用同一规则；除数仍可能为 1 或表可能为空时，不能因此证明下标有效。已合法的 u32 除法继续保留不大于原索引的关系；i32 除法、另一资源及并行独占写另按其原规则检查。

动态长度也可以参与：保存 let n=input.length，在 n!=0 的路径内，item.key%n 取得该输入的索引域；已保存的 view 同样适用。view.length 的上下界随实际引用的长度分量传递，不能移到另一个或重新装载的未知引用。tile 还必须满足每个实际尾块的有效长度，固定宽度不等于每个尾块都有该宽度。活动 lane 的 tile 长度为正，因此以其实际长度取余可直接读取该块。

例如 uint4 的 `values % uint4(17) + uint4(1)` 每个分量均为 1–17，可以直接作为动态整数除数，不需要在源码或执行器中再补零检查。这里推导数值性质，不改变运算、数据权限、GPU 参与集合或阶段边界。48 个未证明／越界／权限错误用法仍实际拒绝；具体运行与机器码对照见 Issues。

该能力不是完整整数关系求解器：浮点区间、一般有符号区间及其转换、已写值的循环不变式／回边与任意算术关系仍未完成。显式整数等式、资源身份与数值区间按实际分量传递；它们只描述当前来源及已证相等快照，不把值类型改成运行时索引句柄，也不替代唯一写所有权。

### 已证明非零的动态整数除余

整数 `/`／`%` 的常量除数继续要求每个分量在编译期非零，u32／i32 及其向量沿用精确常量降低。动态除数支持 **u32／i32 标量及 2／3／4 分量向量**，每个实际除数分量都必须在当前作用域证明非零。向量按分量运算，标量操作数可广播；非标量操作数的维度与整数类别必须匹配。未知或可能为零的任一除数分量仍报 B152。

```c
struct Work { uint3 number; u32 divisor; uint3 quotient; uint3 remainder; };
void divide(inout dataset<Work> data) on compute {
    parallel (item in data) {
        let d = item.divisor;
        if (d != 0) {
            item.quotient = item.number / d;
            item.remainder = item.number % d;
        }
    }
}
```

使用既有值来源捕获实际除数，再由非零比较、bool 命题、选择和常量下界建立证明。条件必须约束当前值；变量改写、错误分支、含未知路径的 OR、过期循环谓词或给另一个数做的非零判断，都不能授权本次运算。标量可在 `while (d != 0)` 的每一轮重新获得证明，循环体改写 d 后，下一轮再按实际条件执行。未选中的选择式分支不执行其中的除余。

向量可逐分量比较后直接整体除余，例如：

```c
struct ComponentWork { int3 number; int3 divisor; int3 quotient; int3 remainder; };
void vector_divide(inout dataset<ComponentWork> data) on compute {
    parallel (item in data) {
        let d = item.divisor;
        if (d.x != 0 && d.y != 0 && d.z != 0) {
            item.quotient = item.number / d;
            item.remainder = item.number % d;
        }
    }
}
```

Proof 沿既有分量来源记录非零信息，构造的拼接／标量广播、swizzle、整数取负和位转换按实际分量传递；OR 的每个结果分量只要对应任一操作数已非零，就能保证非零。选择式按分量保留两侧共同事实。捕获的记录字段及可变局部子字段使用同一份分量映射；局部赋值只更新被改写分量的来源与事实，旧条件不能约束被替换的新值，其余未改写分量仍可使用原有知识。分支合并共有事实，循环入口撤回可能写入的分量；已写分量仍需在循环中按当前条件重新证明，缓存的旧谓词不能代替新条件。

只比较一个分量、用不同分量的 OR 条件、或向量中仍含未证明的分量，都不能授权整体除余。向量 bool 比较结果不是标量条件；可按上例显式合取，或者在源码中定义非零值如 `bits | int3(1)`。这些算术与条件是源程序的实际语义，不是编译器额外生成的保护。非零事实本身不授予资源索引或散写权限，也不生成 CPU／GPU 数据或运行期状态。

u32 的按位 OR 结果不会小于任一操作数，前端保留两侧已知下界的较大者，数值上界按前节的有效位规则计算，不直接复制资源索引身份。例如程序显式计算 `bits | 1` 时，结果的下界为 1。选择式 `d != 0 ? d : 1` 两侧结果均已证明非零，也可直接用作除数；这些是源码自身的数值定义。编译器不生成零检查、异常捕获或替换除数。

有符号分量以非零事实精化当前值，支持 `d != 0`、`d == 0` 的相反分支、严格正／负范围及不含零的常量范围；和已知非零值相等也能传递该事实。缓存 bool、短路合流、按值函数及可变局部沿用同一来源与失效规则。整数取负与 i32／u32 之间保留位值的转换也保留非零信息，但不保留旧值身份或索引域。类型构造消费转换产生的分量证明。u32 数值区间确实排除零时可以推导非零；一般有符号算术、浮点转换与任意关系求解仍不支持。

已有比较关系与端点数值界也可证明当前整数分量非零。例如while(i<count){i+=1;}正常退出后的count<=i，与随后count>0共同排除i=0；if(a>=b&&b>0)与交换条件顺序保持一致。无符号b!=0可作为正下界，有符号的严格正／负范围按实际类型消费。向量除数逐分量成立，常量未选侧的零除仍报B152；覆盖、旧快照、可能归零的缩窄／回绕和break不能沿用失效关系。只消费原非空实际路径及原值身份，不生成运行时零检查，也不宣称一般循环不变式或任意整数关系求解已完成。

已证明非零的 u32 除余不会增大分子，因此分子原有的 dataset／tile／view 索引域可继续传给结果；除法不把精确 length 身份自动变成索引。这不授予 i32 索引域，也没有完成任意动态算术、函数接口显式非零类型或全部数学定义域。

IR 使用同一 IntegerDivide，除数分为 Constant 与 Value。动态除数是实际 SSA 操作数，参与别名改写、公共表达式、活跃性、寄存器／溢出槽分配与 tile 私有状态延续；CPU 批次的统一性同时取决于分子和除数。动态除余保留真实控制流，不能像已知非零常量除法一样提前移出条件分支。

CPU 动态 u32 直接生成无符号 DIV；动态 i32 将两个实际操作数在内部符号扩展为 i64，使用 IDIV64 后消费低 32 位商／余数。四元素批次逐 lane 执行相同规则并合回批次，其他可批处理运算继续使用 SSE2；不声称存在 SSE2 packed 整数除法或统一的向量除法吞吐。

GPU 的常量与动态有符号除余按实际宽度生成同宽无符号幅值，执行 [SPIR-V OpUDiv／OpUMod](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html#OpUDiv) 后恢复符号；幅值可以表示同宽有符号最小值的绝对值，符号恢复按对应位宽运算。商向零截断，非零余数与分子同号；有符号最小值除以 −1 返回同一最小值，余数为 0，恒等式 `n = q*d+r` 按对应位宽成立。CPU 内部宽度和 GPU 幅值运算都不改变源语言类型或数据 ABI，不插入零检查、溢出特判或异常处理，GPU不发射有符号OpSDiv／OpSRem。CPU常量路径保持既有精确倒数／移位与 −1 降低，不调用另一套运行时或 Shader 编译器。

### 商与余数的联合降低

源码继续使用 `/` 和 `%`。同一直线块内，相同实际 SSA 分子与除数的商、余数由既有值转发合并为一个双结果 IntegerDivide；两种书写顺序都可合并。只使用一个结果时，死代码消除会缩成单结果，两个结果均无消费者时删除整个计算。IR 的结果枚举明确表示 Quotient、Remainder 或 Both，结果重编号、活跃性、寄存器／溢出槽与 tile 私有状态共用既有多结果处理。

CPU 动态 u32／i32 除余分别从一次 DIV／IDIV64 取得低 32 位商与余数；四元素批次逐 lane 计算并收集两个输出。常量 u32／i32 除余只计算一次精确商，并以 `n − q*d` 取得余数。源值与除数在两个结果写回之前完成读取，允许分配器按真实活跃性复用位置。既有 signed MIN／−1 的取低位规则保持。

合并仅沿同一块内的真实值，不跨 If、Loop、Break／Continue 或 tile 发布／集合点；分支、循环体内部仍可各自合并。改写分子或除数形成的新值不会与旧值配对，显式保留的不可变快照可以复用。动态非零证明及可提前求值范围保持原规则。GPU由B为两个输出分别编码运算；常量与动态有符号除数由同一发射入口形成同宽无符号幅值，商／余数使用OpUDiv／OpUMod并分别恢复符号，共用本次生成的幅值与符号信息。不承诺驱动生成一条联合除余指令。原生核对与受控 CPU 计时范围见 Issues。

### 条件求值选择式

```c
let i = item.source_index;
let value = i < input.length ? input[i].value : item.value;
item.value = value;
```

条件必须是 bool；两侧仍能继续交付值时必须具有相同类型，已退出分支不参与值合并。结果可以是标量、向量、整记录，也可以是 CPU／GPU 程序中的不可变 view 或含 view 的记录。条件先求值一次，随后只求值选中的表达式。嵌套选择、普通函数内联及函数里的局部循环沿用相同语义。复制 view 只复制已有引用值，真实资源仍由原来的根数据与提交消费者持有。

真／假两侧分别取得条件成立／不成立时的范围事实。例如上例中的读取有效，`i >= input.length ? input[i].value : item.value` 仍报 B137；选择结束后不把某一侧条件作为无条件事实；实际结果分量与该侧来源的关系沿原选择条件保存，后继条件成立时可消费同一被选值的范围。tile length、同源的不可变长度别名，以及保存到 bool 的命题同样适用。不可变 view 也可在其长度条件成立的一侧使用普通索引，外部索引继续使用既有 `unsafe_read` 契约。

即使条件为常量，两侧也都接受类型、访问与能力检查；常量选中值以外的代码不进入最终执行程序。未选侧的整数零除数仍报 B152，类型不一致仍报 B124，不能把未执行等同于不检查源码。

短路与选择表达式复用普通 if 的环境合并。动态条件只保留可能执行路径共有的静态字段／分量事实；编译期已知的条件直接交付实际所选路径的环境及读取／写入／跨元素效果，不清空未写字段，也不把未执行调用算成真实 Count 写入。确定初始化按同一选择关系合并：两侧都写入的 write 输出可以随后读取，动态短路只在右侧写入的位置不能据此变为必然初始化。条件先前取得的值身份、原输入求值顺序和仅求值所选表达式的运行规则保持，没有新增运行时检查。

前端把动态选择转换为既有 If、分量局部槽及合流读取，CPU／GPU 后端继续消费同一 IR；没有另外的运行时选择器、宿主任务或数据副本。外部 dataset、tile、view、光线查询、subgroup、局部循环、浮点计算与转换保留真实控制流，不提前执行。尤其 subgroup 的活动参与者由实际选中的分支决定；其硬件组织见 [Vulkan subgroup 说明](https://docs.vulkan.org/guide/latest/subgroups.html)，不假定设备 lane 顺序或固定宽度。

优化器可将能够安全提前求值的两侧重新变成直接值选择：整数／bool 的运算和比较、非零常量整数除法、位操作、已有值复制，以及由并行域证明有效的当前独占元素读取。它不提前读取任意外部索引，也不扩大协作参与范围。一个 `choose ? (a + 7) / 3 : (b ^ 0x81234567) * 5` 当前元素算例最终为 0 个 If、1 个 Select、0 个局部槽，仍可进入既有直线 map SIMD；这不是所有分支程序的吞吐结论。普通 if 的等价赋值结构也可使用这一优化。合流后的可变局部若仍被后续循环消费，真实存储保留，值转发与 DCE 按实际消费者删除其余存储。

未选侧不引起其中的浮点除法执行，已通过直接原生调用观察未选 0/0 无新增异常状态、选中 0/0 产生 Invalid 状态。本项不是跨线程浮点环境、任意舍入模式或全部 NaN 算术的完整规范；f32 运算的选择不因此进入可提前求值集合。复杂控制流／循环 SIMD 和完整范围推导仍见 C-002／C-005。

## 原生 subgroup 值协作

subgroup 是 GPU 在一个 workgroup 内实际划分的协作集合。Blink 现在直接表达该作用域的值运算，生成封闭 Subgroup IR，再由自研后端编码 SPIR-V；调用者不需要切换到 HLSL／GLSL，也没有逐组的宿主调用或全局中间数组。这些操作要求 计算函数／函数显式 `on gpu`，不把 GPU 的 lane 组织伪装成 CPU 执行语义。

| 源码操作 | 类型与含义 |
|---|---|
| `subgroup_lane()`／`subgroup_size()`／`subgroup_id()` | 返回 u32：物理 subgroup 内 lane、该 subgroup 宽度、workgroup 内 subgroup 编号 |
| `subgroup_elect()` | bool，仅当前活动集合里 lane 编号最小者为 true |
| `subgroup_any(predicate)`／`subgroup_all(predicate)` | bool 标量谓词的投票，返回 bool |
| `subgroup_count(predicate)`／`subgroup_prefix_count(predicate)` | bool 标量谓词的总计数／低于当前 lane 的计数，返回 u32 |
| `subgroup_broadcast_first(value)` | 广播当前活动集合中最小 lane 的值，支持数值或 bool 标量／向量 |
| `subgroup_shuffle(value, lane)` | 读取指定物理 lane 的值；lane 为 u32 标量，可逐 invocation 不同 |
| `subgroup_shuffle_xor(value, mask)` | 从当前 lane 编号 XOR mask 所得的 lane 读取值，mask 为 u32 标量 |
| `subgroup_shuffle_up(value, delta)`／`subgroup_shuffle_down(value, delta)` | 从当前 lane 编号减／加 delta 所得的 lane 读取值，delta 为 u32 标量 |
| `subgroup_sum(value)`／`subgroup_exclusive_sum(value)` | f32／u32／i32／u64／i64标量及现有32位向量的组内和／独占前缀和，返回同类型 |
| `subgroup_min(value)`／`subgroup_max(value)` | f32／u32／i32／u64／i64标量及现有32位向量的逐分量最小／最大值，返回同类型 |
| `subgroup_bit_and(value)`／`subgroup_bit_or(value)`／`subgroup_bit_xor(value)` | u32／i32／u64／i64标量及现有32位整数向量在实际参与集合中的逐分量按位归约 |
| `subgroup_exclusive_bit_and(value)`／`subgroup_exclusive_bit_or(value)`／`subgroup_exclusive_bit_xor(value)` | 当前物理 lane 之前、实际参与者的逐分量独占按位前缀，返回同类型 |

`.h` 接口示例：

```c
struct WorkItem { f32 cost; f32 group_peak; u32 chosen_before; bool keep; bool any; };
void annotate(inout dataset<WorkItem> work) on gpu;
```

`.b` 中的设备算法：

```c
void annotate(inout dataset<WorkItem> work) on gpu {
    parallel (item in work) {
        item.group_peak = subgroup_max(item.cost);
        item.chosen_before = subgroup_prefix_count(item.keep);
        item.any = subgroup_any(item.keep);
    }
}
```

B 后端按原 SubgroupOp 和实际类型发射内建读取、交换、投票和归约等指令。所需设备能力沿原合同交接。同一调用只包含当时实际参与的 invocation：尾组只处理有效元素，分歧分支只处理进入该分支的参与者，循环按各次动态调用的参与者协作。`subgroup_size()` 仍是物理宽度，不是活动数量；活动数量使用 `subgroup_count(true)`。lane 不是 `index()`，subgroup 编号也不是全局任务编号，不能假定相邻记录必然按某种 lane 顺序分组。这些规则遵循 [Vulkan group operations](https://docs.vulkan.org/spec/latest/chapters/shaders.html#shaders-group-operations) 与 [SPIR-V non-uniform instructions](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html#_non_uniform_instructions)。

四种shuffle返回与value相同的类型，接收f32／u32／i32／u64／i64标量、现有32位向量及bool；记录、数组、矩阵和 view 不作为交换值。两个实参各求值一次，再对已有值逐分量编码；参数可来自普通 GPU 值函数，跨模块调用和循环继续使用同一指令及寄存器依赖。它们是寄存器值交换，不执行浮点算术；被交换的 NaN 载荷、有符号零和次正规数保留，bool 使用语言的规范真假值。

调用者必须选取同一次动态调用中实际参与的源 lane。目标越出物理 subgroup、源 invocation 未参与、up 减法越界或 down 加法越界时，结果按 [SPIR-V shuffle 语义](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html#OpGroupNonUniformShuffle) 未定义；编译器不钳制 lane、不改换源值，也不插入运行时判断。例如可先在当前活动集合中取得 `first=subgroup_min(subgroup_lane())`、`last=subgroup_max(subgroup_lane())`，再让各 lane 按算法选择 first 或 last 后交换。若在分歧分支中调用，源也必须进入该分支；不能用物理 subgroup_size 代替活动集合。交换得到的 u32 不自动继承原值的索引证明或唯一写权限。

整数和按 32 位取低位。浮点和的运算次序由实现决定，不承诺与 CPU、其他 subgroup 宽度或另一种归约算法逐位相同；NaN、无穷、带符号零和数学定义域仍按设备运算语义处理，不自动修正数值。

这些操作只交换值，不授予跨元素写权限，不建立设备存储的可见性，也不代替 workgroup／全域阶段屏障。组内 rank 不是全局唯一写索引；`data[subgroup_prefix_count(...)]` 不会自动得到安全散写能力。组内计数也不能签发 `prefix(data,count.value)` 关系，接口 requires／ensures 继续使用既有域生产证明。普通 GPU 值函数可以封装协作并在模块间复用，CPU／compute 调用不相容的 GPU 函数在编译期拒绝。

按位 AND 的单位元为 32 位全 1，OR／XOR 为 0。独占前缀的第一个实际参与者取得单位元；顺序按物理 lane 编号，而不是源记录顺序。仅接受 u32／i32 及其向量，f32、bool、记录和 view 在前端拒绝。自研后端直接编码 [SPIR-V 非一致组按位指令](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html#OpGroupNonUniformBitwiseAnd)，使用既有 arithmetic 能力声明，不引入新的运行框架或 Shader 编译器。

优化器对这些指令处理寄存器依赖、别名重写与无用结果删除，不跨含协作的源阶段做 map 融合，避免改变参与集合。SPIR-V 只声明实际使用的 basic／vote／arithmetic／ballot／shuffle／shuffle-relative 能力；直接／XOR shuffle 和相对 shuffle 分别声明对应能力，不连带申请其他组操作。绑定时由同一设备的真实 compute subgroup 属性决定能否执行，不满足则报告能力错误。普通协作管线不固定 32 或 64 lane，不额外启用排序所需的 subgroup size control／full subgroup。排序仍保留自己的 32 lane 要求。未消费的交换可随既有 DCE 删除，不继续生产能力或空 Map；实参中的真实写效果仍保留。

当前已实现 subgroup 值协作与任意 lane shuffle、类型化 tile 输入快照和中间值发布；Device 作用域的 u32／i32／u64／i64 存储原子已实现；块内原子共享值和显式根块屏障现已实现；非原子任意共享写与一般组间协作仍未实现。

## 类型化 tile 输入快照

`tile_input(dataset 或字段, width)` 表达阶段入口的只读分块输入。它必须位于 parallel 开头的 let 声明区，width 为编译期可知的正 u32 值；同一阶段的所有 tile 使用相同宽度。分块按逻辑元素顺序划分，与宿主任务批次及 GPU subgroup 的物理 lane 映射无关。同一 `.b` 算法可以声明 `on compute`，分别由自研 CPU／GPU 后端执行。

`.h` 接口仍声明正式数据和访问权限：

```c
module example.center;
struct Cell { float3 position; };
void center(inout dataset<Cell> cells) on compute;
```

`.b` 中可原地更新每块元素，读取的仍是进入该阶段时的快照：

```c
module example.center;
void center(inout dataset<Cell> cells) on compute {
    parallel (cell in cells) {
        let positions = tile_input(cells.position, 128);
        float3 total = tile_reduce_sum(positions);
        cell.position = total / f32(positions.length);
    }
}
```

`positions.length` 是当前块的实际有效长度，尾块不补成公开的虚假记录；`positions.lane` 是当前元素在逻辑块内的索引。可以用 `positions[positions.lane]` 或 for 游标读取，`ordinal(cursor)` 的结果也可索引同一阶段的其他 tile。常量索引必须在所有可能的尾块中有效；全局 `index()` 不属于 tile 索引域，tile lane 也不因此成为全局可写索引。保留普通值类型、结构体字段和向量分量访问，不暴露裸共享地址或赋予快照写权限。

快照可以来自当前 inout 数据，也可以来自其他可读数据；后者的容量必须覆盖阶段域绑定容量。固定形状不足在编译期报 B200，动态容量由原有 CPU／GPU 绑定边界拒绝。含资源 view 的记录不能作为快照捕获；可以捕获不含资源的值字段。加载出的普通值可以传给函数，tile 绑定本身目前不能作为 `.h` 参数、结构体字段或返回值逃逸。

物理存储按实际 TileLoad 的源绑定和源 word 合并：整条记录视图及字段视图引用同一源位置时，只生产一份共享值；未使用的捕获和字段不占存储。若阶段含有 subgroup 运算，声明宽度仍会影响参与集合，不能随未读取的快照一起删除；保留分块宽度不等于分配无消费者的数据。CPU 范围函数使用按字段打包的局部输入，每个逻辑块先复制必要数据，再执行该块的元素计算，不为每个元素创建任务或复制全数据集。CPU 任务范围按 width 对齐，只有最后一块可以不满。

GPU 在有效元素分支之前由整个 workgroup 共同生产快照，随后执行一次统一控制／内存屏障；尾块中无效 invocation 不读全局记录，仍参与屏障。共享存储只读，阶段主体可以有分歧和局部循环，不需要在这些控制流里手写屏障。屏障覆盖 Workgroup 和 UniformMemory，后者包含 PhysicalStorageBuffer，保证同块的输入读取先于原地写入；依据 [Vulkan memory model](https://docs.vulkan.org/spec/latest/appendices/memorymodel.html) 和 [SPIR-V OpControlBarrier](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html#OpControlBarrier)。

width 同时决定该 map 管线的 workgroup 大小；设备绑定按真实 invocations、X 大小及显式共享字节上限决定能否执行，不把任意合法源码宽度冒充每台 GPU 都支持。`GpuProgram`／`GpuTemplate::with_partition_elements` 对 tile 阶段按整块取整，prefix 参数生成与多维索引也使用同一宽度，计数保持设备驻留。普通无 tile 的 map 继续使用默认 64，集合运算的已有 64 线程实现不变。tile 与 subgroup 可在 `on gpu` 阶段组合，二者的 lane 和宽度具有不同含义。

输入快照的生命周期止于当前阶段；后续阶段或重复步重新取得输入。优化器保留含 tile 协作的阶段边界，消除无消费者捕获，并从最终指令派生 map 的实际读写集合；`.layout` 列出 width、快照与显式共享载荷。当前支持连续一维分块输入，尚未开放任意 gather 映射或跨块 halo；可变原子 tile 与根块屏障见下节。浮点计算继续遵循各后端运算语义，不承诺所有算法逐位一致。

## 类型化 tile 中间值发布

`let name = tile_publish(value, width)` 把当前元素计算出的普通值发布给同块元素。每个有效元素只生产自己 lane 的一份值；编译器完成全块生产与同步后，后续语句才能读取这个不可变版本。它可以发布标量、向量和不含 view 的结构体，不要求存在 tile_input，也不申请新的全局 dataset。

例如对每块变换后的向量求中心，再写回各自元素：

```c
void center(inout dataset<Cell> cells, f32 scale) on compute {
    parallel (cell in cells) {
        let transformed = tile_publish(cell.position * scale, 128);
        float3 total = tile_reduce_sum(transformed);
        cell.position = transformed[transformed.lane] - total / f32(transformed.length);
    }
}
```

输入快照和发布值都可直接交给下文的块内集合运算，归约结果继续是当前元素的普通值。

发布声明当前只允许位于 parallel 或 cooperative 根块，不能出现在分支、循环或普通值函数中；发布值的计算可以使用这些控制流及已有普通函数。一个阶段内的所有输入与发布使用相同的编译期正 u32 宽度，错误位置／形式报 B203，非法宽度报 B199，含 view 的发布值报 B204。发布 tile 使用已有 length、lane、有界 for 游标、ordinal 与索引证明；不能被写入、当成普通值逃逸或提升为全局写权限。普通值函数保持逐 lane 语义；块内阶段可由下文的 cooperative 函数封装。

可以连续发布多个版本，后续版本可以依赖此前的发布值与私有局部状态；此前版本仍可读取。版本生命周期止于本源阶段，不跨 计算函数 phase 或独立执行保留。发布是块内阶段切换，不能当作全域同步或任意组间协作；跨块依赖仍由 计算函数 阶段及执行框架负责。尾块只发布有效元素，原有 prefix 关系、设备驻留计数与分区边界继续适用。

编译器的同一份 IR 同时描述发布位置、字段和值依赖。后向活跃性分析处理分支、局部循环、break 与 seek，只保存后续阶段需要的私有寄存器／局部槽；共享 word 则按最后读取阶段分配。输入字段继续按源绑定／word 去重，发布字段按版本区分，无读取字段不占共享存储。旧 word 的最后读取阶段严格早于新发布阶段时才复用物理槽；仍在本阶段读取的 word 不允许被较快 lane 提前覆盖，不为复用增加另一道用户屏障。

CPU 在一个原生范围函数内，依次让本块所有 lane 执行每个阶段，按 lane 保存跨阶段所需私有值，完成一个块后复用局部存储；不创建逐元素／逐阶段系统任务或全数据集镜像。当前这些流位于范围函数栈帧，仍受原生 ABI 和线程栈资源约束；tile 协作阶段尚未进行跨元素 SIMD。

GPU 保持一个内核：每阶段只让有效 lane 执行源码和发布写入，所有 invocation 随后参与统一控制／内存屏障；无效尾 lane 不读取真实记录或发布数据。跨阶段的寄存器值通过正确前驱的 SSA phi 延续，源局部变量继续为 invocation 私有，不把整个私有帧复制到共享／全局存储。屏障与 phi 分别遵循 [OpControlBarrier](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html#OpControlBarrier) 和 [OpPhi](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html#OpPhi)，没有引入第三方 Shader 编译器或一次发布一次宿主提交。

无消费者的发布可消除；若 subgroup 运算仍能观察阶段切换，则保留其同步作用域而不生产无用载荷。`.layout` 列出逻辑宽度、实际共享值、阶段数及共享字节。实际设备能力继续在已有绑定边界处理，不以合法源码宽度冒充任意 GPU 都有足够线程／共享资源。tile_atomic 与根块屏障沿用这套阶段与布局；当前尚不支持循环内动态发布、非原子任意共享地址写入和跨块 halo；浮点运算仍不承诺跨后端逐位一致。

## 块内原子共享值与显式屏障

`let bins = tile_atomic(value, width);` 在本逻辑块内建立可变共享值。每个有效 lane 用自己的 value 初始化对应元素，初始化与整块发布沿用 tile_publish 的同一 IR、共享布局和同步步骤；返回后可通过原子操作访问本块任意已证明有效的元素。width 是块宽度，不是另一份全局 dataset 容量；尾块的 length 仍为实际有效元素数。已有 tile 范围时可以省略 width，cooperative 函数必须继承调用方范围。

```c
struct Bin { u32 count; u32 sum; };
struct Cell { u32 key; u32 value; u32 count; u32 sum; };
void local_histogram(inout dataset<Cell> cells) on compute {
    parallel (cell in cells) {
        let bins = tile_atomic(Bin(0, 0), 128);
        u32 key = cell.key % bins.length;
        atomic_add(bins[key].count, 1, relaxed);
        atomic_add(bins[key].sum, cell.value, relaxed);
        tile_barrier();
        cell.count = atomic_load(bins[bins.lane].count, acquire);
        cell.sum = atomic_load(bins[bins.lane].sum, relaxed);
    }
}
```

共享元素只通过前述11种atomic操作访问：CPU与GPU原生映射均接受u32／i32／u64／i64标量字段。GPU64位tile必须在真实逻辑设备创建时声明shaderSharedInt64Atomics；64位dataset使用独立shaderBufferInt64Atomics，同一程序实际使用两类目标时分别满足，普通64位存储不要求原子特性。已核验的AMD驱动32.0.21036.18仍存在64位Workgroup OpAtomicLoad降成32位读取的问题；设备报告支持不替代实际正确性。嵌套记录与向量可选择具体标量；普通读取／赋值／复合赋值和直接 tile 集合读取均报 B214，其他原子目标类型报 B215。需要进入只读集合时，先在同步后显式 atomic_load，再 tile_publish 该普通值。它仍不是裸地址，不能逃逸为普通值、全局资源或函数返回的 tile；局部的有界索引、lane、length、游标与字段映射复用原检查器，不授予全局写入或前缀域能力。

`tile_barrier();` 只允许位于已有 tile 范围的 parallel／cooperative 根块，不接收参数、不返回值。不允许放在分支、循环或普通值函数内，即便分支条件暂时是常量；非法位置／参数报 B219，作为表达式报 B218。有效尾 invocation 执行源码，无效尾 invocation 不访问记录或共享元素，但 GPU 中它们同样经过全部整块屏障。当前支持的是静态根块阶段，不是循环内动态 barrier、跨 workgroup 屏障或调度公平性承诺。

原子性和稳定快照是两件事。barrier 之后的一次 atomic_load 可看到其他 lane 后续写入；acquire 不冻结共享状态。若函数必须给所有 lane 返回相同的阶段快照，需要先完成生产，再把读取结果保存在私有值中，最后同步，之后才允许下一阶段写入。例如 `.h` 声明 `cooperative u32 accumulate(atomic tile<u32> bins, u32 value) on compute;`，实现为：

```c
cooperative u32 accumulate(atomic tile<u32> bins, u32 value) on compute {
    atomic_add(bins[0], value, relaxed);
    tile_barrier();
    u32 total = atomic_load(bins[0], acquire);
    tile_barrier();
    return total;
}
```

调用方用 `cooperate accumulate(bins, value)`，参数与实参必须同为 atomic；只读借用不能升级，atomic 借用也不能伪装成普通只读 tile。允许多个 atomic 参数借用同一实际 tile 或字段，操作仍只通过显式原子发生。不同存储身份和逐字段 word 映射保留，不复制共享内容，宽度由调用方专门化。普通值函数仍不接收 tile，动态协作未实现。

IR 的 AtomicTarget 区分 Dataset 与 Tile，操作、结果、显式内存序与寄存器活跃性共用。GPU 的 dataset 地址保持 Device scope=1／UniformMemory；tile 地址使用已有 Workgroup 存储、scope=2／WorkgroupMemory，同一 shader 可同时操作两者。根块屏障复用已有 OpControlBarrier，Workgroup 执行／内存作用域覆盖当前逻辑块；不为一次屏障创建宿主提交。语义依据见 [SPIR-V 原子和屏障规范](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html#OpControlBarrier)及 [Vulkan 内存模型](https://docs.vulkan.org/spec/latest/appendices/memorymodel.html)。

CPU 按完整 tile 对齐领取范围，在原生范围函数的线程私有帧内持有共享值，逐阶段执行所有有效 lane。64位tile值保留原低／高32位lane流，按实际两个字位置装入完整64位寄存器，完成原子运算并写回原流；同一范围内没有并发CPU消费者，u64／i64仍按完整位宽回绕、比较并返回旧值，不将两条流当作连续64位内存。CPU原子操作共用同一发射分派；32位tile在原位置使用MOV、无LOCK的XADD／CMPXCHG及普通极值／按位读改写，交换使用普通读取／写入，保留旧值返回。该存储不会被其他CPU线程访问，无需LOCK、主机锁或宿主线程屏障；dataset仍使用原有LOCK XADD／CMPXCHG和内存XCHG，真实范围所有权决定这一编译期区别。两端允许合法的不同争用次序，不能要求逐行票号或获胜者相同。

同一IR生命周期布局根据真实Scalar／Ty投影实际单元：CPU保持原32位字流，GPU的普通窄值进入u32共享平面，u64／i64／f64的完整原始位进入u64平面；逻辑低／高字对应同一64位单元，不将一份值复制到两个共享平面。固定字段按实际消费者紧凑分配，动态聚合保持声明字节步长及必要间距。原子读与写都计入同一最后访问阶段，初始化字段按实际访问保留；仍在本阶段访问的单元不能与新发布覆盖同一槽。显式屏障参加同一阶段切分和私有值活跃性，无载荷屏障仍保留同步职责。类型化非原子任意共享写、动态同步、浮点原子、有界并行预留与完整并发工作队列尚未实现；GPU64位tile的当前原生缺陷仍按上述Issues记录。

## 块内归约与独占扫描

`tile_reduce_sum(tile 或数值字段)` 返回当前逻辑块所有有效元素之和，同块元素各得到同一总值。`tile_scan_exclusive_sum(tile 或数值字段)` 返回当前 lane 之前的有效元素之和，lane 0 为 0；顺序由逻辑元素顺序定义，与物理 subgroup lane 映射无关。`tile_reduce_min`／`tile_reduce_max` 返回同块有效元素的最小／最大值，`tile_scan_exclusive_min`／`tile_scan_exclusive_max` 返回当前元素之前的极值。这些操作返回普通数值，支持u8／i8／u16／i16／u32／i32／u64／i64／f32／f64标量及现有32位2／3／4分量向量，按分量独立计算；结构体需要选择数值字段，支持嵌套字段与向量 swizzle。不对 bool、记录或资源直接归约或扫描。

整数位集合另有 `tile_reduce_bit_and`／`tile_reduce_bit_or`／`tile_reduce_bit_xor`，以及对应的 `tile_scan_exclusive_bit_and`／`tile_scan_exclusive_bit_or`／`tile_scan_exclusive_bit_xor`。它们接受u8／i8／u16／i16／u32／i32／u64／i64标量及现有32位整数向量，返回相同类型；AND 单位元为全 1，OR／XOR 为 0，独占扫描不包含当前元素。输入、发布、字段借用、cooperative 函数与尾块沿用同一逻辑元素范围；不把物理 subgroup 宽度变成 CPU／GPU 的公共数据顺序。

```c
parallel (cell in cells) {
    let input = tile_input(cells.position, 128);
    float3 total = tile_reduce_sum(input);
    let before = tile_scan_exclusive_sum(input);
    let centered = tile_publish(input[input.lane] - total / f32(input.length), 128);
    float3 centered_total = tile_reduce_sum(centered);
    cell.position = before + centered[centered.lane] + centered_total;
}
```

这些原语当前只能作为 parallel 或 cooperative 根块的 let／普通值类型声明初始化式，且只有一个 tile 参数；分支／循环、普通值函数或其他表达式内部调用报 B205，数据集／普通值参数及不支持的数值类型报 B206。需要后续算术时先保存原语结果。显式值类型仍必须匹配。返回值不带有 tile 索引或全局唯一写入证明：任意数值前缀可能相同或溢出，不能直接当作安全散写目标；也不能签发全域 prefix(data,count.value) 关系。

归约／扫描和发布共用一个块内阶段序列及私有值活跃性。CPU 在原生范围内对每块、每个实际使用分量顺序计算一次，向后续 lane 交付归约值或前缀；工作量随有效元素数线性增长，不让每个 lane 再遍历一遍全块。GPU 将该分量装入共享树，向上归约，扫描再向下传播；树操作复用全域扫描的同一自研 SPIR-V 核心。使用 [Blelloch 的归约／前缀树](https://www.cs.cmu.edu/afs/cs.cmu.edu/project/scandal/public/papers/CMU-CS-90-190.html)，有效合并操作与共享访存量为 O(width)，同步深度为 O(log width)，不额外要求固定 subgroup 宽度或 subgroup arithmetic 能力。

非二次幂的逻辑宽度在 GPU 工作区内部补到下一个二次幂，最多不到两倍；尾部、补齐槽不成为可索引的源元素。无效尾 invocation 不读取源 tile，仍参与统一同步。求和及前缀和继续逐分量复用单流工作区；极值向量的实际结果分量共用同一轮树同步，各有一个工作流。工作区为 `next_power_of_two(width) * 4 * K` 字节，K 取同阶段实际需要的最大流数：和为 1，极值为未被消除的结果分量数。完成一批分量的读取后，后续集合才可复用工作区。它与尚在被读取的不可变 tile 数据分开；真实 GPU 资源边界检查数据及工作区总量。CPU 不分配 GPU 共享树，只保留必要的私有结果流。

未读取的局部写入、寄存器结果与集合载荷通过同一份后向活跃性消除；前端不再因创建了一个局部变量，就强迫它的归约、输入读取和共享工作区常驻。若后续 subgroup 仍能观察阶段同步，则只保留作用域，不生产无人读取的载荷或工作区。循环终止及可见写入的职责保持原样，不把运行时保护加在下游。

u32／i32 和仍取低 32 位，整数扫描可跨后端精确核对。f32 的 CPU 顺序累加与 GPU 树形累加具有不同结合顺序和内部单位元，不承诺逐位相同、NaN 载荷一致或任意输入的固定误差界；数学定义域与数值稳定性仍是算法责任。原生非二进制浮点样本的实际误差独立记录在 Issues，不修改输出值来伪造一致性。

极值扫描的首元素使用对应单位元：

| 类型 | min 单位元 | max 单位元 |
|---|---|---|
| u32 | 4294967295 | 0 |
| i32 | 2147483647 | -2147483648 |
| f32 | +Infinity | -Infinity |

例如给每个元素交付所属块的包围盒角点：

```c
parallel (vertex in vertices) {
    let positions = tile_input(vertices.position, 128);
    float3 lower = tile_reduce_min(positions);
    float3 upper = tile_reduce_max(positions);
    vertex.lower = lower;
    vertex.upper = upper;
}
```

这是当前元素自己的结果，不产生每块一条输出记录或新的散写权限。全部有效元素都参与；尾块只使用实际长度。CPU 为每个分量顺序计算一次并交付结果；GPU 上行树按左／右子树顺序合并，独占扫描下行先合并左侧前缀与当前子树。f32 相等或比较无序时交付右操作数；没有有效右子树时直接交付左子树，填充值不能覆盖实际 NaN。NaN 使该二元操作不具备一般结合律，因此 CPU 与 GPU 的不同结合顺序仍可产生不同结果；不承诺含 NaN 的归约逐位相同。

块内集合只作用于一个 tile，不是下文的全域集合，不增加全局中间 dataset 或宿主提交；当前已实现下面的静态协作函数，尚未实现循环内动态协作、通用扫描运算符或分段集合。`.layout`的`tile_word_bytes`表示IR原32位字流的CPU块内布局，`collective_scratch_bytes`单列集合工作区。GPU Shader发射者将各入口实际声明的Workgroup数组字节相加，原ExecutionStage.shared_bytes保存同一阶段各主／尾／排序等入口的峰值；运行库按这一编译映像字段核对真实设备容量，CLI直接导出为`gpu_stage=..., shared_bytes=...`，不另算一份预测预算。

## 协作函数与 tile 借用

`.h` 用 `cooperative` 声明块内协作效果，`tile<T>` 参数借用调用方已有 tile 的只读值与块范围，`atomic tile<T>` 则借用显式原子访问能力；两种模式不能互换。普通值与整域计算使用同一函数定义；cooperative 仍明确声明块内协作效果，接口与实现必须一致。调用处使用 `cooperate`，只能放在 parallel／cooperative 根块的局部声明中；同步效果不隐藏在普通值函数调用里。

例如 `.h`：

```c
module blocks;
cooperative float3 centered(tile<float3> input, float3 value) on compute;
```

对应 `.b`：

```c
module blocks;
cooperative float3 centered(tile<float3> input, float3 value) on compute {
    float3 total = tile_reduce_sum(input);
    let result = tile_publish(value - total / f32(input.length));
    return result[result.lane];
}
```

消费者在自己的模块声明 `import blocks;`，在 parallel 中调用：

```c
let positions = tile_input(cells.position, 128);
let value = cooperate blocks::centered(positions, cell.position);
cell.position = value;
```

每个 cooperative 接口至少有一个 tile 参数，所有借用属于调用方同一逻辑块。元素类型固定，宽度随调用方专门化；这不是完整元素类型泛型。同一函数无需按 17／128／1024 等宽度复制实现。函数内 `tile_publish(value)`／`tile_atomic(value)` 继承范围，不能用显式宽度建立隐藏的固定宽度要求（B209）。计算函数 已有 tile 范围后也可省略发布宽度；首次建立范围仍须使用 tile_input 或显式发布宽度，缺少范围报 B209。

参数可借用整条记录、嵌套字段或 swizzle，包括重复分量；类型与逐分量来源一起传递。借用只引用原 tile 及 word 映射，不复制输入快照或另建 dataset。length、lane、foreach 游标和 ordinal 沿用原块的有效长度及有界索引语义。只读 tile 不能存进普通变量／结构体、作为 计算函数 参数或函数返回值，也不能包含资源 view；它不是一般设备地址或可写对象。调用者在函数返回后继续持有自己的 tile。

函数可以调用普通值函数，也可在根块嵌套 `let result = cooperate other(...);`，支持普通值和记录返回。普通值参数及返回值沿用既有值语义。函数内部可有局部循环、分支、continue 和 seek；协作调用、发布和块内集合本身仍只在根块，不能放进这些控制流或内嵌算术／return 表达式。index() 仍只属于 计算函数 的 parallel；函数通过借用的 lane／游标或显式值参数处理索引。CPU／GPU 执行目标相容性与不允许递归的规则继续适用。

类型检查函数库时，编译器保存真实的符号块范围及借用来源，不填入假宽度或伪造数据集。在 计算函数 调用处，参数视图绑定到实际来源，发布宽度继承为具体值；封闭执行 IR 只包含具体布局和既有发布／集合／普通指令。cooperative 函数静态内联到同一个原生范围／GPU 内核，不增加函数任务、宿主调度或 GPU 提交。函数自己的根块、循环和游标上下文与调用方分开，值与借用通过参数传递；共用 IR 的局部值转发、x64 物理寄存器分配及可表达的普通直线及统一控制流逐元素阶段的 SIMD 已接入，tile 协作阶段仍按标量 lane 执行。

跨模块函数必须在该模块 `.h` 导出，并由使用方直接 import；限定名不能绕过接口。层级模块沿用点分路径，例如 `cooperate lib.blocks::centered(...)`。未导出的同模块辅助函数可以在本模块内使用。`bc check`／`blanguage::check` 能检查不含 计算函数 的类型／函数库，同时检查 `.h/.b`、完整导入闭包和未调用函数。原生 build／`blanguage::compile` 仍需要公开 计算函数；库检查成功不表示已经产出可执行内核。

```powershell
.\target\release\bc.exe check ..\BLEngine\Programs\math\field.b ..\BLEngine\Programs
```

借用类型位置错误报 B207，将 tile 当作普通值报 B145；非法协作位置／效果报 B208；接口效果不一致报 B028；借用元素类型错误报 B124，未导出／未直接 import 的调用报 B158，递归报 B160，设备不相容报 B161。当前未实现控制流内动态协作、递归、tile 返回、一般高阶函数、完整泛型及独立二进制函数链接。

## 阶段与集合运算

```c
void step(inout dataset<BLBody> bodies, f32 dt, f32 strength) on compute {
    integrate(bodies, dt, strength);
    energy(bodies);
}
```

前端证明安全时，相邻同域 map 可融合，减少中间全局读写。局部消除后退出的空map不再阻挡原本合法的邻接融合；其承载的显式phase顺序交给下一真实阶段，不能因空并行退出而丢失。显式完整语句 `phase;` 保留阶段边界；它表示全域阶段依赖，不是某个线程任意执行的 workgroup barrier。阶段语句按词法后的 `phase` 与 `;` 识别，二者之间可有空白或注释；合法名称 `phase` 的普通赋值、复合赋值或调用不产生这个边界。实际允许阶段语句的位置仍由原阶段与参与规则决定。

```c
struct Total { f32 value; };

void total_energy(read dataset<BLBody> bodies,
                     write dataset<Total, 1> total) on compute {
    reduce_sum(bodies.energy, total.value);
}
```

### 数值全域归约

`reduce_sum`／`reduce_min`／`reduce_max` 接受f32／u32／i32／u64／i64标量及现有32位2／3／4分量向量，按分量独立计算。输入为 read dataset 的数值字段，也可使用具有既有域证明的 `prefix(dataset,count.field)`；字段可嵌套。目标为独立可写、形状 1 数据集中的同类型字段。目标记录可以同时拥有其他字段，归约只写指定字段的实际分量，保留其余存储与 ABI 填充。bool、整记录、资源、不同数值类型／向量宽度在编译期拒绝，不授予普通 parallel 任意写入能力。

```c
struct Vertex { float3 position; };
struct Bounds { float3 lower; float3 upper; };
void bounds(read dataset<Vertex> vertices,
               write dataset<Bounds, 1> result) on compute {
    reduce_min(vertices.position, result.lower);
    reduce_max(vertices.position, result.upper);
}
```

| 运算 | f32空域单位元 | u32 | i32 | u64 | i64 |
|---|---|---|---|---|---|
| sum | +0 | 0 | 0 | 0 | 0 |
| min | +∞ | 4294967295 | 2147483647 | 18446744073709551615 | 9223372036854775807 |
| max | −∞ | 0 | −2147483648 | 0 | −9223372036854775808 |

整数sum按标量实际宽度保留低32或64位。浮点 min／max 以严格比较选择左值，否则取右值，包含相等、有符号零和无序比较时的右值；不隐式过滤 NaN。CPU 按本次范围划分依次累计，再合并范围结果；GPU 组内按 32／16／8／4／2／1 的距离合并，再递归归约组结果。不同分组可能改变浮点和的舍入及特殊值的结果，不能承诺 CPU／GPU 全部逐位一致。GPU 极值不合并没有真实元素的右侧填充，单元素 NaN 不会被尾部单位元替换；空域仍按表输出单位元。f32 阶段沿用既有 DenormPreserve／SignedZeroInfNanPreserve 能力要求，整数归约不增加浮点设备要求。

全域归约使用单一 Reduce IR，类型和算子由前端产生；块内与全域的 sum／min／max 及整数位集合共用 CollectiveOperator 与 GPU 分量合并代码。CPU 自研叶范围函数在一次遍历中累计所有分量，不逐元素调用 Rust 函数、不使用局部栈帧；第五个原生参数交付范围结果地址，运行框架负责合并部分结果。参数位置遵守 [Windows x64 调用约定](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention?view=msvc-170)，源语言调用接口不暴露这个执行细节。

GPU 每组 64 线程、最多 1024 字节归约共享存储，各分量共用同步层。递归中间记录紧密保存数值分量，不复制整份目标记录；只有最终目标绑定使用原记录布局及字段偏移。有效前缀的实际数量由设备间接参数交付，目标布局由当前绑定的结果域决定，不能用有效数量提前缩到单组来推断已经绑定最终输出。所有部分结果与原有间接参数由同一提交持有并留在显存，不中途读回或由 CPU 计算 GPU 归约结果。

`bc build` 导出主／递归 tail SPIR-V 及原生 x64。集合阶段沿用既有依赖、prefix requires／ensures 与跨提交所有权；输出形状和权限、数据绑定及生命周期仍由原边界负责。自定义普通值归约见下节，分段归约见后文；跨块邻域和设备自主任务尚未实现。

### 自定义普通值归约与投影归约

归约不再只接受固定算子。用户用普通 `.h/.b` 值函数定义单位元、合并规则及可选投影；编译器将函数静态编进 CPU 范围函数与 GPU 共享树。用户函数按调用点静态展开，数学内建保持既有链接，不通过宿主逐元素回调或另一套表达式解释器执行聚合。

```c
// .h 声明以下类型与函数；函数体放在 .b。
struct Vertex { float3 position; };
struct BoxValue { float3 lower; float3 upper; u32 count; };
struct Result { BoxValue bounds; };

BoxValue box_identity() {
    return BoxValue(float3(1.0 / 0.0), float3(-1.0 / 0.0), 0);
}
BoxValue point_box(float3 point) {
    return BoxValue(point, point, 1);
}
BoxValue join_box(BoxValue a, BoxValue b) {
    return BoxValue(min(a.lower, b.lower), max(a.upper, b.upper), a.count + b.count);
}
void bounds(read dataset<Vertex> vertices,
               write dataset<Result, 1> result) on compute {
    reduce_map(vertices.position, result.bounds, point_box, box_identity, join_box);
}
```

`Record(field0,field1,...)` 按声明字段顺序构造记录，支持嵌套记录与向量；各字段类型必须完全相同，不将异构分量统一转成一个标量类型。需要数值转换时显式写 f32／u32／i32 构造。记录构造只组合已有值寄存器，不分配对象，不复制 ABI 填充。字段数量错误报 B157，类型错误报 B124；值来源与逐分量证明沿既有规则传递。

`reduce(source.field,target.field,identity,combine)` 聚合相同普通值类型。identity 是无参数、返回 T 的函数，combine 是 `(T,T)->T` 的函数；标量、向量、混合f32／u32／i32／u64／i64／bool的记录都可使用。目标仍为形状 1 的独立可写数据集，只写目标字段的逻辑分量，其他字段及记录内／外填充保持。空域返回 identity。零尺寸普通记录同样是合法T；其结果没有物理分量，其他字段与填充保持，仍按原单位元、投影及合并语义执行。

`reduce_map(source.field,target.field,project,identity,combine)` 允许源 S 与结果 T 不同，project 的签名为 `(S)->T`。投影在主阶段处理有效源元素，直接进入累积／共享树；不物化 N 条 T 的中间数据集。递归摘要只合并 T，不再执行 project。输入可以是现有 `prefix(data,count.value).field`；前缀计数留在 GPU，原 Count 所有权与模块 requires／ensures 继续生效。返回记录中的普通 count 只是数值，不自动授予有效域或任意散写能力。

函数引用可以是静态名称、已导入的限定名称或下一节的 `bind` 参数绑定。只接受 `on compute` 的普通值函数，参数和返回值必须匹配；可调用其他普通值函数，使用局部记录改写、分支、循环、向量及已有数学／整数运算。资源引用不能成为聚合值，不能捕获 dataset／acceleration、使用 index()、subgroup／tile 或执行 计算函数。集合调用实参数量报 B169，函数角色／形参数量／设备／名称形式报 B212，未知或未导出函数报 B158；函数自身仍接受完整模块、返回类型、非零／范围与副作用检查。静态类型和访问证明不等于已经证明任意用户算法的数学性质。

combine 必须在算法使用的值域中满足结合律，identity 必须是双侧单位元；编译器不运行样本来猜测这些性质，不增加运行时检查。实现保持逻辑输入的左右顺序，允许改变结合括号，所以无需交换律；矩阵组合、仿射变换、稳定候选筛选等有序结合运算可直接表达。浮点也使用真实运算和既有保留模式，结合顺序变化仍可能改变舍入、零符号及 NaN／无穷结果，不能把 f32 加法当作逐位结合或承诺固定误差。数值域及稳定性属于算法责任。

CPU 的单位元、投影、逐范围累积和范围摘要合并都由自研 x64 编码执行，跨调用值和跨迭代累积值参加同一寄存器／溢出槽活跃性分析；最终只按类型偏移写出结果。GPU 按相邻子树从左到右合并 64 路输入，每次静态展开都拥有独立 SSA 结果，空右子树直接保留左结果；纯合并器中的数据相关循环不引入屏障。共享载荷为64乘结果Ty.size字节，内部摘要按Ty实际大小／字段偏移保存物理u32字；一个64位逻辑分量占两字。原生值函数仍按逻辑分量绑定寄存器，不把内部填充当作字段读写。真实设备能力在既有编译资源边界绑定，根源／目标继续使用各自记录ABI并保持填充。

这些运算复用原归约的阶段、递归参数、双摘要缓冲、原生 Fence 与存储所有权，不创建另一套调度器。`bc build` 导出主／递归 SPIR-V、CPU 主／`.tail.x64` 及各自 reloc；递归 CPU 函数同样支持必要的数学链接。自定义扫描与分段归约见后文；动态任务、完整泛型及自动证明结合律仍未实现。

### 集合值函数的执行参数绑定

`bind(function,capture0,...)` 把当前 计算函数 的统一普通值追加到函数的元素参数之后。该形式适用于 `reduce`、`scan_exclusive`、`scan_segmented_exclusive`、`reduce_segmented` 及各自 `_map` 的 project／identity／combine 位置，三种函数可以各自绑定不同数量与类型的参数：

| 角色 | 未绑定的元素参数 | 最终函数签名 |
|---|---|---|
| project | 一个 S | `(S,C0,...)->T` |
| identity | 无 | `(C0,...)->T` |
| combine | 两个 T | `(T,T,C0,...)->T` |

```c
// .h 声明同样的函数和 program；.b 定义函数体。
BoxValue local_box(float3 point, float3 origin) {
    let local = point - origin;
    return BoxValue(local, local, 1);
}
void local_bounds(read dataset<Vertex> vertices,
                     write dataset<Result, 1> result,
                     float3 origin) on compute {
    reduce_map(vertices.position, result.bounds,
               bind(local_box, origin), box_identity, join_box);
}
```

上例的 BoxValue／Vertex／Result 与单位元、合并器沿用前节。源几何只保留原坐标；坐标系作为一次执行的统一参数传递，不复制到 N 条输入。identity 和 combine 也可绑定记录规则，例如选择最大／最小候选及同分时的顺序。同一个已编译程序可以在后续执行更换这些值，不需要按参数值重新编译。

捕获表达式支持标量、向量、嵌套普通记录、显式类型构造、已声明常量、统一算术／值函数结果和 `dataset.length`。length 仍是该绑定的容量／长度元数据，不是设备生产的有效 Count；不能通过绑定表达式读取 `count[0].value` 或其他数据集元素。资源、元素游标、index()、tile／subgroup、计算函数 和隐藏资源副作用不属于该绑定的值语义。函数主体仍只能通过显式形参取得绑定值，不能隐式访问调用方变量。

第一个参数为静态函数名或显式泛型引用，例如 bind(project<T,Mode>,offset)，不能是函数调用或另一份 bind；`bind(f)` 等同于没有额外参数的静态引用。角色、设备和形参数量错误报 B212；普通类型仍由 B124 检查；读取数据集元素／非统一普通值报 B213，资源名称及非法作用域沿用已有诊断。bind 只能出现在这些集合函数参数位置，在 parallel／普通函数中当作一般值求值报 B156。cfn原生C函数地址签名值及其CPU实际调用按本规范对应章节使用。一般编译期函数组合与泛型能力沿真实消费者推进，目标和捕获关系应可静态确定；不将通用堆闭包、类型擦除后的动态函数对象列为必须补齐的能力。普通拥有记录与静态函数可继续表达实际状态及其操作，不要求引入闭包运行时。

前端沿用 计算函数 根参数解析、普通值检查和静态内联；捕获生成同一 ValueCode 中的 Uniform／普通值 IR，未使用参数由既有值流与死代码消除删除。x64 与 SPIR-V 在主遍历、摘要、前缀及结果传播中直接消费这份 IR；没有宿主逐元素回调、N 条捕获副本或另一套编译表示。封闭集合现已把无条件路径上只依赖统一参数／常量的纯计算提至原生入口，复用相同表达式；CPU 每个范围一次，GPU 每次 invocation 一次。它不意味着一次 计算函数 或每个 workgroup 只计算一次；用户分支／循环中的计算及元素相关运算仍留在各自位置，通用循环不变量外提尚未完成。结合律、双侧单位元及有效数值域必须对本次绑定的参数成立，仍由算法定义。

### 静态泛型算法参数

集合算法可直接接收 `identity<uint3,7>`、`combine<7>`、`compose<Affine<uint3,Mode>>` 等显式泛型函数引用。支持普通归约、独占扫描、分段扫描、分段归约及各自的投影形式；投影、单位元与合并器共用原值函数参数入口。普通函数和 计算函数 的调用仍写括号，例如 `identity<uint3,7>()`；带括号的调用结果不能作为静态函数引用。

完整 `.h` 示例：

```c
module typed_scan;
const u32 ChosenBias = 7;
struct Local { uint3 value; u32 audit; };
struct Global { uint3 before; u32 audit; };
T identity<T: numeric, const Bias: u32 in {0,7}>();
T plus<T: numeric, const Bias: u32 in {0,7}>(T left, T right);
T project<T: numeric, const Bias: u32 in {0,7}>(T value, T offset);
void entry(read dataset<Local> local, inout dataset<Global> global) on compute;
```

对应 `.b`：

```c
module typed_scan;
T identity<T: numeric, const Bias: u32 in {0,7}>() { return T(0); }
T plus<T: numeric, const Bias: u32 in {0,7}>(T left, T right) { return left + right; }
T project<T: numeric, const Bias: u32 in {0,7}>(T value, T offset) {
    return value + offset + T(Bias);
}
void entry(read dataset<Local> local, inout dataset<Global> global) on compute {
    scan_exclusive_map(local.value, global.before,
        bind(project<ChosenBias>, uint3(3)),
        identity<uint3,ChosenBias>, plus<ChosenBias>);
}
```

project 和 plus 只给 const 后缀，类型由源值、合并器的实际元素参数或真实 bind 参数推导；identity 没有普通参数，因此显式给出 uint3。目标字段的期望类型不会反向推导没有输入的泛型参数。显式实参规则与普通调用相同：完整类型和值清单，或全部 const 后缀；省略尖括号的普通函数名仍沿原推导规则。

嵌套的具体记录、用户类型集合、记录值参数、跨模块限定名和外层函数／计算函数 的类型与 const 参数均可用于引用。模块 const 先解析为实际值，引用实例沿唯一原声明生成；原生代码只留下实际运算。bind 的后续参数是实际统一数值，不承担代替缺失类型实参的职责；不创建每元素闭包、运行期函数表、间接调用或新调度任务。

引用必须解析为可见的 compute 普通值函数；全部形参仍是普通按值参数，输入／输出类型、参数数量、访问、捕获统一性与原有 requires 在同一调用入口检查。直接引用、显式引用与 bind 共用 ValueCode，已有有效前缀、数据所有权、阶段及集合结合顺序保持。未使用的非法类型／值实例也仍在编译期拒绝；结合律与双侧单位元属于算法责任，编译器不以样本猜测它们。

显式引用当前仅用于这些集合算法及其 bind 首参数。它不能保存到局部／字段、作为普通值返回、传给一般值参数或参与数值运算；解析为引用后按普通值求值报 B236，其他非法语法沿原解析入口拒绝。引用角色／设备／形参数量沿原 B212，bind 非统一读取沿 B213，普通类型 B124、模块可见性 B158、泛型实参 B231／B232／B235／B015 保持。cfn原生C函数地址签名值、普通参数／返回和CPU实际调用与这里的静态引用分别定义；系统回调的代码／状态保留寿命仍待实现。更多静态高阶组合按明确目标和捕获关系设计，不以通用动态lambda、堆闭包或对象虚派发替代这一目标。

### 集合入口的不变计算

集合内核在原有值 IR 上分出入口、单位元、投影与合并段。无条件路径上的 Constant／Uniform、普通一元／二元算术、转换、数学内建、Select 和固定非零除数的整数除余，只有全部操作数都来自入口纯值时才前置。元素值、Index、资源读写、局部槽及用户 If／Loop 不前置；动态整数除法仍留在其非零证明成立的作用域。单位元／投影／合并都没有新增能力或放宽访问证明。

入口沿用现有值流／表达式消除，同一统一值与有序表达式只保留一份；不改浮点运算顺序，不用代数近似或交换左右操作数。三个函数体分别继承入口值，不能从另一个尚未执行的函数体取得局部结果。每个后端阶段只生成它实际需要的角色：摘要不再投影，普通扫描与分段摘要传播不计算不用的单位元；真正需要段头重置的最终分段遍历仍保留单位元。

CPU 自研范围函数在循环前计算这些值，以既有结构化活跃性求得跨迭代输入，再由原寄存器分配器保存跨调用的值及必要溢出槽。GPU 在共享树之前计算入口 SSA，后续每次合并仍为元素结果产生新 SSA，只保留入口定义；用于有效 Count 的间接启动参数内核不执行集合值函数入口。没有引入宿主缓存、预计算缓冲、额外提交或运行时开关。

这是封闭集合的入口外提，不是完整 LICM、动态任务系统或各类代码的统一加速保证。CPU 是每个分区范围计算一次；GPU 是每次 shader invocation 计算一次，不能称为整次提交或整个 workgroup 只有一份计算。元素投影本就每 invocation 执行一次，GPU 收益主要来自多个合并节点复用表达式及减少重复指令，实际吞吐仍需以程序测量。

### 自定义普通值独占扫描与融合投影

```c
// .h 声明类型与函数；.b 定义函数体与 program。
struct Transform { u32 scale; u32 bias; };
struct Local { Transform value; };
struct Global { Transform before; };

Transform identity() { return Transform(1, 0); }
Transform compose(Transform a, Transform b) {
    return Transform(b.scale * a.scale, b.scale * a.bias + b.bias);
}
void chain(read dataset<Local> local,
              write dataset<Global> global) on compute {
    scan_exclusive(local.value, global.before, identity, compose);
}
```

`scan_exclusive(source.field,target.field,identity,combine)` 对每个有效输入位置 i 写入 `[0,i)` 的有序合并，首项是 identity。上例按模 32 位组合一维仿射变换，左右顺序不能交换；实际浮点变换、矩阵和其他普通记录同样通过用户值函数表达。它是并行集合操作，程序不需要管理线程、摘要数组或主机／设备回调。

`scan_exclusive_map(source.field,target.field,project,identity,combine)` 接受 `(S)->T` 的投影，直接把源值转换到局部扫描，递归与传播只处理 T，不重复投影，也不生成逐输入投影中间数组。其余函数签名、纯值访问、模块导入、结合律和双侧单位元要求与自定义归约相同。浮点结合顺序及数学链接继续遵守上节语义，不保证逐位一致。

源必须可读（read或inout），结果为可写的相同T字段；同记录字段按下文真实分量判断重叠，支持标量、向量和混合类型嵌套记录。目标容量至少容纳源容量，可以额外保留尾部；静态不足报 B188，动态容量仍由既有参数绑定边界检查。只写目标字段的逻辑分量，其他字段、填充及有效域之后的尾部保持。空输入不写任何输出记录。不能覆盖源值／段头的实际分量、改写输入前缀 Count，或把资源引用作为聚合值。

源可为已有证明的 `prefix(data,count.value).field`。操作直接产生相同 Count 对应的 `prefix(target,count.value)`，可供后续 parallel、集合或 requires／ensures 接口消费。没有新的 Count、计数读回或按实际数量动态分配；仍按容量预留递归层级和存储。

CPU 首遍按范围产生局部独占结果及紧密 K-word 摘要；自研 x64 前缀函数原地扫描少量范围摘要，第二遍 x64 合并范围前缀与局部结果。实际范围数量来自同一次输入划分，与每份摘要字数分别交接；T为零尺寸普通记录时K为0，摘要地址步长也为0，不生产虚拟数值分量，原有效位置、其他字段与容量尾部语义保持。三部分复用同一值 IR、寄存器分配及 Windows ABI，数学调用跨迭代值仍由活跃集持有；Rust 不解释用户合并器。GPU 复用既有 64 路扫描调度，以有序共享树上扫、根置单位元及下扫产生局部独占值，递归摘要与逐级传播保持 `combine(前置摘要,局部值)`。共享存储为 `64*K*4` 字节；没有真实输入的右子树不参与上扫。所有摘要与有效数量留在设备，临时资源沿原提交寿命释放。

`bc build` 导出主、`.tail.spv`、`.add.spv`、必要的 `.root-add.spv`，以及 CPU 主、`.prefix.x64`、`.add.x64` 和各自数学 reloc。运行时直接消费编译产物对象，不依赖导出文件形成第二份状态。自定义分段扫描／归约见后文；完整泛型仍需实现。

### 自定义普通值分段独占扫描

```c
// Transform、identity、compose 沿用上节；.h 声明接口，.b 定义函数体。
struct SegmentItem { Transform value; bool head; };
void chains(read dataset<SegmentItem> local,
               write dataset<Global> global) on compute {
    scan_segmented_exclusive(local.value, local.head,
                             global.before, identity, compose);
}
```

`scan_segmented_exclusive(source.field,source.head,target.field,identity,combine)` 在每段内部计算独占前缀。有效域首项隐含开始一段；后续 true 开始新段，连续段头各自形成单元素段。段头输出 identity，不包含本项；没有显式段头的非空域就是一个连续段，空域不写输出。它适合多条连续存放的有序链，不隐含任意树形依赖或通用任务图。

`scan_segmented_exclusive_map(source.field,source.head,target.field,project,identity,combine)` 先按 `(S)->T` 投影，直接进入分段扫描；摘要与传播只处理 T，不重新投影。纯函数、类型、容量、左右顺序、结合律／单位元及浮点语义与普通自定义扫描一致。源可以是已有证明的 prefix；段头跟随该有效域，筛选丢弃的段头不再开始新段。扫描继续交付同一 Count 的输出 prefix，不建立第二份有效数量。

段头必须为同一个 read 源记录的 bool 字段，不能来自另一个数据集或不相同的有效前缀。类型／函数签名／作用域继续在编译期检查；权限／域不匹配报 B170，缺少或失效的前缀关系报 B191，静态目标容量不足报 B188。目标和输入 Count 独立；只写目标字段的逻辑分量，其他字段、ABI 填充与有效域后的容量保持。

分段摘要为 `(T 的尾段值, 是否含段头)`，紧密保存 K+1 个 word。左右摘要 `(a,h)`、`(b,j)` 合并为 `(j ? b : combine(a,b),h||j)`；右侧已经开始新段时直接保留其尾值，不执行跨段用户合并器。摘要的单位元为 `(identity(),false)`。这是一种有序结合运算，继续使用相同 ValueCode 与扫描层级，没有逐输入段号或额外映射数据集。

CPU 主范围函数遇到段头后重置累积；原生摘要前缀函数直接合并／重置摘要，传播函数到范围内首个段头即结束。需要跨迭代保留的单位元和累积值进入既有活跃集，数学调用遵守相同 ABI。GPU 在原共享树中处理 K+1 分量，上扫与下扫保持逻辑顺序，根阶段输出当前段头的单位元。根摘要最后一个 word 同时编码首个段头的 lane+1，零表示没有段头；递归将非零解释为含段头，最终传播只修改该工作组首个段头之前的值。共享存储为 `64*(K+1)*4` 字节，所有摘要、有效计数和递归间接参数留在设备，资源仍由原提交持有到最后消费。

导出文件与普通自定义扫描相同，`bc build` 的 CPU prefix／add 也包含需要的数学链接。它没有增加另一个运行时、同步协议或调度器。用户定义分段归约见下节；动态协作／任务和完整语言仍未完成。

### 自定义普通值分段归约

```c
// BoxValue、point_box、box_identity、join_box 沿用普通投影归约示例。
struct Point { float3 position; bool head; };
struct BoxResult { BoxValue value; };
struct Count { u32 value; };
void segment_boxes(read dataset<Point> points,
                       write dataset<BoxResult> boxes,
                       write dataset<Count, 1> segments) on compute
    ensures prefix(boxes, segments.value) {
    reduce_segmented_map(points.position, points.head,
                         boxes.value, segments.value,
                         point_box, box_identity, join_box);
}
```

`reduce_segmented(source.field,source.head,target.field,segments.value,identity,combine)` 按逻辑顺序为每个非空段输出一个 T。`reduce_segmented_map(...,project,identity,combine)` 在原始 S 与聚合 T 之间使用纯值投影。有效域首项隐含新段，后续 true 开始新段，连续段头各自形成单元素段；空输入交付 segments=0，不改写目标记录。目标按原段顺序排列，字段以外的记录内容、填充和实际段数之后的容量保持。

源值与 bool 段头必须来自同一 read 记录和有效域；目标、输出 Count、源和输入前缀 Count 独立，输出 Count 是形状 1、单 u32 字段的可写数据集。目标容量至少为源容量，容纳最坏情况下每项一段；静态不足报 B188，动态容量沿原参数绑定边界检查。函数签名、纯值与资源访问、逻辑左右顺序、结合律／双侧单位元、浮点差异与其他自定义集合相同。

输出 `prefix(target,segments.value)` 由此操作直接产生，可经 ensures／requires 交付后续 parallel、集合或独立提交。Count 来自编译器生成的真实段计数；聚合 T 内部的普通 count 字段不能自行签发域关系。整个操作不会按段数在设备动态分配存储。

摘要为 `(T 的尾段值,含段头标记,段头数量)`，紧密占 K+2 个 word。右摘要有段头时直接取其尾值，否则调用用户合并器；标记取 OR，数量相加。CPU 第一遍原生范围函数产生摘要，原生前缀函数原地合并摘要、写出总段数，第二遍原生范围函数在真实段尾写结果。范围参数仍复用原摘要存储，包含进入范围前的 T、总有效长度与已出现段数；没有宿主解释合并器或逐元素回调。

GPU 第一遍只写工作组摘要，递归扫描及反向传播只处理这些摘要，最后一遍直接写各段尾的唯一结果位置与实际段数。使用同一有序共享树，显式共享为 `64*(K+2)*4` 字节；源值、有效数量、段数及所有摘要留在设备，不中途读回。两端都遍历源两次，投影在这两次原始源遍历中执行，递归摘要不再投影；这是用源重算避免 N 条投影／段号中间数组的实现取舍，不代表所有负载的最优吞吐。投影仍必须是无资源副作用的纯值函数。

`bc build` 导出主、tail、add、finish SPIR-V、必要的有效域 dispatch，以及 CPU 主、prefix、finish x64 和各自数学 reloc。运行框架共用既有分段归约的递归命令、扫描缓冲及提交寿命，不创建另一套调度器。完整泛型、动态协作／任务仍未实现。

### 整数按位全域归约

```c
struct Item { uint4 flags; };
struct Masks { uint4 common; uint4 combined; uint4 parity; };
void masks(read dataset<Item> items,
              write dataset<Masks, 1> result) on compute {
    reduce_bit_and(items.flags, result.common);
    reduce_bit_or(items.flags, result.combined);
    reduce_bit_xor(items.flags, result.parity);
}
```

三种运算接受同类型的u32／i32／u64／i64标量及现有32位2／3／4分量向量，逐分量保留实际32或64位。AND 交付所有输入共有的位，OR 合并出现过的位，XOR 交付各位的奇偶性。空域AND返回对应位宽的全1（u32为0xffffffff、u64为0xffffffffffffffff，有符号为−1），OR／XOR返回0；输入全 0、全 1、负数和最高位都按位处理，不引入浮点或 bool 数值转换。

输入为只读数值字段或已有证明的 `prefix(dataset,count.field)`，输出为独立形状 1 的可写数据字段。目标其余字段与填充保持；f32、bool、整记录、资源以及类型／向量宽度不一致仍在既有类型入口拒绝。筛选后的有效域可直接归约，容量尾部不参与运算。CPU 与 GPU 按相同位运算得到一致结果；这不外推浮点归约的舍入与 NaN 语义。

CPU 原生范围内直接使用整数 AND／OR／XOR 指令，运行框架以相同算子合并范围结果。GPU 复用既有 64 路归约树、递归中间页和有效域的设备参数，块内扫描复用现有共享存储树；没有新增数据副本、提交协议或计数读回。位集合可用于可见性、资源类别和任务标记，不能代替原子存储操作、全域屏障或写入所有权。

### 类型化全域独占扫描

```c
struct Count { u32 value; };

void prefix(read dataset<Count> input,
               write dataset<Count> output) on compute {
    scan_exclusive_sum(input.value, output.value);
}
```

输入／输出字段支持同类型的 u32／i32 标量及 2／3／4 分量向量。`scan_exclusive_sum`、`scan_exclusive_min`、`scan_exclusive_max`、`scan_exclusive_bit_and`、`scan_exclusive_bit_or`、`scan_exclusive_bit_xor` 分别计算当前元素之前所有元素的和、极小、极大及按位 AND／OR／XOR，逐分量返回同类型。空输入不产生元素，首项为对应单位元：

| 操作 | u32 单位元 | i32 单位元 |
|---|---|---|
| sum／bit_or／bit_xor | 0 | 0 |
| min | 4294967295 | 2147483647 |
| max | 0 | −2147483648 |
| bit_and | 4294967295 | −1 |

整数和保留低 32 位，i32 极值使用有符号比较；按位操作保留原始 32 位。不同分组得到相同整数结果，需要计数不溢出的算法仍须选择合适的数据规模。字段可以位于记录中，其他字段及 ABI 填充保持原值；不再要求输出记录只有一个 u32。输入／输出仍独立，输入是 read，输出为 write/inout。完整域要求输入／输出长度相等，有效前缀允许输出容量大于输入容量。形状不满足报 B186，bool／记录／资源及字段类型不同报 B175，可写输入或别名报 B170；动态容量继续由既有参数绑定边界负责。

```c
struct Work { uint4 units; };
struct Offsets { u32 id; uint4 begin; uint4 minimum; uint4 flags; };
void offsets(read dataset<Work> input,
                inout dataset<Offsets> output) on compute {
    scan_exclusive_sum(input.units, output.begin);
    scan_exclusive_min(input.units, output.minimum);
    scan_exclusive_bit_or(input.units, output.flags);
}
```

扫描是全域集合操作，不能放入单元素 parallel 或当作纯函数调用。前端以类型化 Scan 保存字段、分量与算子，保留既有阶段边界和依赖。CPU 普通独占扫描的两遍 x64 范围机器码由 B 发射。第一遍按实际布局写局部独占值，经与归约共用的第五个参数交付范围摘要。执行框架按同一算子合并摘要，第二遍沿原范围前缀并行修正输出字段。叶范围保存其实际使用的非易失寄存器，并交接 Windows 展开信息，无局部栈分配。

GPU 每组 64 线程执行 work-efficient Blelloch 扫描，各分量共用同步层，普通扫描最多使用 1024 字节共享树。有效尾组的全部线程参加必要的屏障，域外位置贡献对应单位元；递归摘要和前缀紧密保存真实分量，不复制整份输出记录，也不读回 CPU。最终传播使用原目标记录步长与字段偏移；它们与紧密摘要布局不同时，复用现有根输出传播 pass，相同时使用同一传播内核，不额外建立重复管线。三分量向量的记录填充与其他字段同样保留。提交继续持有全部临时页到真实完成。

稳定 compact 与键排序的内部计数仍使用同一扫描实现中的 u32 求和定义；各自的布尔规范化、稳定散写和计数所有权保持。有效前缀扫描交付原 Count 对应的输出域，后续可直接读取该前缀；容量尾部不参与扫描，也不被修改。GPU 额外绑定的扫描页表属于既有执行框架，不增加源参数或提交协议。`bc build` 输出主 `.spv`、`.tail.spv`、`.add.spv` 和 CPU `.x64`／`.add.x64`，最终布局不同或分段时另输出 `.root-add.spv`。浮点、用户普通值及分段扫描见相应章节；设备生成新任务仍未实现。

### 浮点全域独占扫描

`scan_exclusive_sum`／`scan_exclusive_min`／`scan_exclusive_max` 接受 f32／float2／float3／float4，逐分量返回当前元素之前的和／极小／极大，首项分别为 +0.0／+Infinity／-Infinity，空域不写元素。完整域、有效前缀、独立输入／输出、嵌套记录字段及容量尾部规则与整数扫描相同。浮点按位扫描仍报 B175；连续及分段数值扫描共用原 Scan IR，分段行为见下节。

```c
struct Light { float4 energy; bool selected; };
struct Size { u32 value; };
struct Distribution { u32 id; float4 before; };

void energy_prefix(read dataset<Light> queue,
                      read dataset<Size, 1> size,
                      inout dataset<Distribution> output) on compute
    requires prefix(queue, size.value)
    ensures prefix(output, size.value) {
    scan_exclusive_sum(prefix(queue, size.value).energy, output.before);
}

void selected_energy(read dataset<Light> input,
                        inout dataset<Light> queue,
                        inout dataset<Size, 1> size,
                        inout dataset<Distribution> output) on compute {
    compact(input, input.selected, queue, size.value);
    energy_prefix(queue, size, output);
}
```

输入的逻辑顺序定义前缀包含哪些元素，浮点结合顺序由实现决定。CPU 范围内使用 SSE 标量加法，范围结果由同一执行框架合并再原生传播；GPU 使用既有 64 路 Blelloch 树、递归摘要及前缀传播。CPU叶范围函数保存和恢复其实际使用的非易失步长寄存器，各分量按真实源／目标布局共同遍历记录，其他字段与填充保持。浮点摘要和整数摘要共用物理页与原有提交所有权，不增加宿主中间读回或逐元素任务。

GPU 主扫描、递归扫描、摘要传播与最终记录传播均声明既有 `DenormPreserve 32` 和 `SignedZeroInfNanPreserve 32`，并沿用设备能力绑定。求和直接生成 SPIR-V `OpFAdd`，不转换为整数加法或调用外部 Shader 编译器。CPU／GPU 不承诺逐位相同、固定误差界、NaN 载荷一致或中间结果不会溢出；次正规数、无穷和 NaN 继续参与真实运算，不截断、替换或滤除。对严重消减或精确分布有要求的算法仍须处理数值稳定性，编译成功不意味着这些数学条件成立。

浮点极值使用既有严格比较：左值更小／更大时取左值，相等或无序时取右值，保留右侧有符号零及 NaN 位模式。CPU 使用 MINSS／MAXSS，GPU 使用有序比较与位选择；尾组没有真实右元素的子树直接保留左值，跨范围／组传播的首项直接取得此前真实前缀，不让空的局部前缀或尾部填充改写 NaN。含 NaN 的二元极值不具有一般结合律，CPU 范围划分与 GPU 树形分组可能产生不同结果；该能力不承诺含 NaN 的任意输入逐位相同，也不为此增加过滤、替换或运行时检查。

### 原生分段独占扫描

```c
struct Work { u32 amount; bool head; };
struct Offset { u32 value; };

void local_offsets(read dataset<Work> work,
                      write dataset<Offset> offsets) on compute {
    scan_segmented_exclusive_sum(work.amount, work.head, offsets.value);
}
```

每个 `head=true` 元素开始新段，该元素的输出为算子单位元；其后元素只组合本段在它之前的值。首个元素隐含开始第一段，允许其 head=false。连续段头各自产生单位元，空输入不产生记录。上面的 sum 示例中，amount 为 `[2,3,5,7,11]`、head 为 `[false,false,true,false,true]` 时，结果为 `[0,2,0,5,0]`。整数加法按低 32 位计算。此操作可为已按逻辑顺序分组的任务生成段内偏移、边界极值和类别标记；段长可以超过 CPU 范围、GPU 工作组和物理数据页。

| 操作 | 源与目标字段类型 | 单位元 |
|---|---|---|
| `scan_segmented_exclusive_sum` | u32／i32／f32 及 2／3／4 分量向量 | 整数 0，浮点 +0.0 |
| `scan_segmented_exclusive_min` | u32／i32／f32 及其向量 | 整数最大值，浮点 +Infinity |
| `scan_segmented_exclusive_max` | u32／i32／f32 及其向量 | 整数最小值，浮点 -Infinity |
| `scan_segmented_exclusive_bit_and` | u32／i32 及其向量 | 全 1 |
| `scan_segmented_exclusive_bit_or`／`scan_segmented_exclusive_bit_xor` | u32／i32 及其向量 | 0 |

数值向量按分量独立扫描，同一 bool 段头同时重置所有分量。源与目标数值字段类型必须相同，可位于不同记录的嵌套字段中；段头必须为同一 read 源记录的 bool 字段，输出为独立可写记录，保留其余字段与填充，并遵守同一扫描形状规则。段头字段可以直接写 `work.head`；源值使用 `prefix(work,size.value).amount` 时，段头自动使用同一有效域，也可以明确写相同的 `prefix(work,size.value).head`。不同数据源或不同前缀报 B170，类型不同或浮点按位算子报 B175，缺少域生产关系报 B191。操作只出现在 计算函数 全域阶段，不能在 parallel 内抢号或作为逐元素值调用；非法位置报 B156。

编译器以 `Scan` IR 保存源／目标字段、数值类型、算子及可选段头字段，依赖、资源绑定、接口 requires／ensures 和跨提交域所有权继续消费同一份阶段信息。有效域输出交付 `prefix(offsets,size.value)` 关系，容量尾部保持原内容；计数与中间结果不为调度读回 CPU。

分段算法的结合元为 `(各分量尾段值, 是否含段头)`：组合左结果 a 与右结果 b 时，若 b 含段头，逐分量采用 b 的尾段值，否则以对应算子合并；段头位取逻辑或。K 个数值分量的摘要紧密占 K+1 个 word，数值分量后保存段头信息，由同一 `Scan.summary_words()` 交付后端与物理分配。CPU 两遍 x64 范围机器码由 B 发射。第一遍扫描范围内所有分量，经普通归约／扫描共用的第五个结果指针写摘要。执行框架合并摘要，第二遍并行修正各范围首个段头之前的输出，遇到段头直接结束。两遍保存 R12 和 R13 并交接 Windows 展开信息，无局部栈分配。

GPU 每组 64 个 invocation，在 `64*(K+1)*4` 字节 workgroup 存储中完成所有分量与段头的上行／下行扫描，直接写入目标字段。K 为 1–4，显式共享载荷为 512–1280 字节；每条全局摘要为 8–20 字节。根层将首段头位置加一编码在摘要最后一个 word 中：有段头时仅首段头 invocation 写编码，没有段头时仅最后一个物理 lane 写 0。两种写入互斥，尾组与空输入同样适用；递归只消费其是否非零，不另存段头副本。组间前缀仅修正首段头之前的结果，不生成逐元素全局标记或末尾复制程序。树的 workgroup 屏障与既有计算阶段的全局内存屏障分别负责自己的消费者，存储类作用域遵循 [Vulkan memory model](https://docs.vulkan.org/spec/latest/appendices/memorymodel.html)。

浮点求和沿用上述 f32 运算，极值沿用严格比较与右值选择，两者使用原设备保留模式；同一段的 CPU 顺序、范围组合和 GPU 树形结合顺序不同，不承诺逐位一致、固定误差界或 NaN 载荷相同。新段分别从 +0.0／+Infinity／-Infinity 开始，不把上一段的 NaN／无穷前缀带进新段。浮点按位扫描仍不开放；整数按位操作不等于原子或全域屏障，数值前缀不签发任意散写权限。

`bc build` 对内置分段扫描导出主 `.spv`、递归 `.tail.spv`、摘要修正 `.add.spv`、根输出修正 `.root-add.spv` 和 CPU `.x64`／`.add.x64`。执行框架直接持有编译结果与全部临时资源到实际完成。用户自定义扫描与分段归约见前文；设备自主分配输出仍未实现，内置分段归约见下节。

### 原生分段归约

```c
struct Sample { uint4 value; bool head; };
struct Segment { uint4 value; };
struct Count { u32 value; };

void summarize(read dataset<Sample> input,
                  write dataset<Segment> output,
                  write dataset<Count, 1> count) on compute
    ensures prefix(output, count.value) {
    reduce_segmented_sum(input.value, input.head, output.value, count.value);
}
```

每个段返回一条数值结果，按源段顺序写入 output，count 是实际段数。首条有效输入隐含段头，不要求 head 为 true；后续 true 开始新段，连续段头各自形成单元素段。空输入产生 count=0，目标记录保持原内容。没有 head 时整个非空输入只有一段。

`reduce_segmented_sum`／`min`／`max` 接受同类型 u32／i32／f32 标量及 2／3／4 分量向量；`reduce_segmented_bit_and`／`bit_or`／`bit_xor` 接受整数及其向量。逐分量运算与全域归约共用算子和单位元；整数加法取低 32 位。f32 分段极值归约已开放，f32 极值扫描仍未开放。浮点结合顺序由实现决定，求和可能不同；极值沿用有序比较、相等或 unordered 时取右值的规则，含 NaN 时重组会改变结果，不承诺跨后端相同或 NaN 载荷一致。实际非有限值继续参与运算，不作替换。

源数值与 bool 段头必须来自同一个 read 数据集；数值也可写为 `prefix(input, size.value).value`，段头跟随同一实际有效域。目标字段类型必须完全相同，目标、输出 Count 与源／输入 Count 独立，Count 仍为形状 1 的单 u32 字段。目标容量需要容纳输入容量的最坏段数，当前不会按实际段数在设备动态分配输出。静态形状不足编译报 B188，动态形状在现有资源绑定边界拒绝。权限／别名、类型、Count 形状、参数形式、作用域与有效域不足分别沿用 B170／B175／B171／B172／B156／B191。

编译器直接生产 `prefix(output,count.value)` 关系，可立即用于后续 parallel、归约、扫描、压缩、排序，或经 requires／ensures 交付独立消费者。它不从普通数值 Count 猜测关系，也不授予用户任意散写权限。段尾的唯一输出位置由这个封闭集合操作生成；只改写输出的数值字段，其他字段、填充及有效段数之后的容量尾部保持。后续消费者仍遵守各自的访问能力。

实现只保存范围／工作组摘要，不先生成逐输入记录的扫描结果或段号数组。摘要为 `(K 个尾段分量, 是否含段头, 段头数)`，紧密占 K+2 个 word；若右摘要含段头，尾值取右，否则合并左右尾值，标记取 OR、计数相加。

CPU 两遍 x64 范围机器码由 B 发射。首次遍历生成各范围摘要，执行框架在原摘要存储中合并并交付范围前缀、总有效长度和进入范围前的段数。第二遍直接在真实段尾写唯一结果，跨范围只读原源的下一条段头。两个叶范围保存 R12 和 R13 并交接 Windows 展开信息，无局部栈分配。空域由同一执行阶段写 Count=0。

GPU 首次遍历只写组摘要，递归扫描与传播只处理摘要，最后一次遍历读取源与组前缀并直接写段尾；没有逐元素全局中间数组或原子计数器。64 路共享树显式占 `64*(K+2)*4` 字节，即 768–1536 字节；临时全局存储从 ceil(N/64) 个摘要开始递归。无效右子树不参与归约，填充身份值不改写实际尾值。首个有效 lane 拥有组摘要写入；最后一条有效输入写总段数，空域由首组写 0，矩形间接启动补齐的无效组不取得输出权限。

完整域／有效域共用 SegmentedReduce IR、阶段依赖和既有范围／Vulkan 提交所有权。GPU 的有效计数、摘要与所有递归启动参数留在设备；命令及临时存储仍按容量预留，不代表设备已经能动态构造任意任务图。`bc build` 导出主、tail、add、finish SPIR-V，以及 CPU 主函数和 `.finish.x64`。自定义普通值／投影分段归约见前文；稀疏集合、动态协作及设备自主任务仍待完成。

### 稳定压缩筛选

```c
struct Item { float3 position; u32 id; bool keep; };
struct Count { u32 value; };

void select(read dataset<Item> input,
               write dataset<Item> output,
               write dataset<Count, 1> count) on compute {
    compact(input, input.keep, output, count.value);
}
```

`compact(input, flags.keep, output, count.value)` 按输入顺序复制谓词为 true 的完整记录，计数输出为选中的记录数；空输入和全 false 都产生 0。有效结果只包含输出的前 `count.value` 条记录，尾部不属于筛选结果；当前实现保留其原内容，不能把默认全零尾部当作筛选语义。记录值和 ABI 填充字节均原样复制，不重新求值或量化。GPU scatter 与稳定排序的收集共用 B 的整记录复制。源和目标分别沿实际页绑定解析一次记录起点，再按各记录原对齐使用 8／16／32 位物理标量读写。页边界只落在记录之间。bool 与浮点存储编码保持，不在搬运中规范化或运算。

谓词字段必须为 bool，计数字段为 u32、数据集形状为 1 且只有一个字段。输入／输出记录类型完全相同，谓词的元素数与输入一致，输出容量至少为完整输入数。输入和谓词是 read，可以使用输入记录自己的 bool 字段，也可以使用独立 read 数据集。输出和计数为 write/inout，它们彼此以及与读资源不能别名。静态谓词形状不符报 B187，输出容量不足报 B188，记录类型不符报 B189，权限／别名报 B170；动态实际形状在既有绑定边界检查。当前仍要求最坏情况下足够的输出容量，不会根据一次实际选择率猜测分配量。

CPU 用自研 x64 范围计数与稳定复制：先并行计数，计算少量范围前缀，再并行复制每个范围的所选记录。GPU 将 bool 归一为 0/1，重用独占扫描层级；每条被选记录取得唯一的前缀序号，最后一条输入写出总数，空输入单独写出 0。扫描、散写和计数全部由同一提交持有资源，全程不读回 CPU。源程序没有获得任意散写权限。

`compact` 是全域集合阶段，保留前后依赖；可以与 map、scan、sum 同属一个程序。`bc build` 对该阶段额外输出 `.tail.spv`、`.add.spv`、`.scatter.spv` 和 CPU `.scatter.x64`。普通 parallel 仍表示绑定容量；使用下面的 prefix 域直接消费 GPU 有效计数。

### 原生有效前缀并行域

```c
void process(read dataset<Item> input,
                inout dataset<Item> output,
                write dataset<Count, 1> count) on compute {
    compact(input, input.keep, output, count.value);
    parallel (item in prefix(output, count.value)) {
        item.id += 100;
    }
}
```

`prefix(dataset, count.field)` 是执行域，表示前 count 条有效记录；不是普通值函数或对整个容量逐元素执行后再判断。`index()`／`ordinal(item)` 为该前缀内从 0 开始的序号；dataset 本身的长度仍表示绑定容量。当前类型作用域必须持有该数据与计数的有效关系：它来自本函数的 compact／分段归约、接口 requires，或已调用模块的 ensures。计数一经改写，对应关系立即失效；未生产、被改写或来自另一输出的计数均报 B191。这些关系只存在于编译器，不新增运行期计数状态。write 计数可用于此执行域，因为对应集合生产者已证明其初始化；不放宽任意 write 数据的读取权限。

每个元素仍只拥有自身写入权限。计数读取进入阶段读集合和跨元素依赖；只有底层数据和 prefix 计数字段都相同的 map 才可按既有条件融合。完整容量域与有效域不能融合，显式 phase 仍保留。前缀域不授予任意地址散写权限；原子计数返回的数字也不自动取得写所有权或有效域生产证明。

CPU 直接读取自己存储中的已生产计数，按该长度执行原生范围函数。GPU 用 B 发射提交私有的层级执行快照。每层 16 字节，前三个 u32 为间接启动 X/Y/Z 工作组数，最后一个为该层有效数量。生成器一次解析参数表的物理页地址，随后沿实际记录偏移写入。Vulkan 用 `vkCmdDispatchIndirect` 消费同一页的记录。map 使用一层，sum／scan／compact 一次产生覆盖当前 u32 域的六层参数；排序沿首层实际组数确定直方图域。筛选的后续参数直接使用前序工作的 NumWorkgroups X/Y。计数与启动参数不为调度读回 CPU，命令和所有临时资源仍由同一提交持有到完成。原始 Count 数据集仍是生产者的输出，执行快照不是可独立修改的另一份权威计数。

X 分区按实际工作组宽对齐，并受设备上限与 `with_partition_elements` 控制；Y/Z 根据实际组数展开。B 发射 u64 多维索引，先与有效数量比较，再提供语言层 u32 index。集合先用组首索引判断整组是否属于有效域，使矩形补齐组退出，同时保留空域写出单位元或计数所需的首组。空前缀的 map 产生零维启动。间接参数缓冲区具有真实 INDIRECT_BUFFER 用途，参数写入到间接读取／Shader 读取之间具有明确内存依赖；同一提交的后续时间步覆盖参数前也等待此前读者。ABI 与用法依据 [Vulkan 间接启动规范](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdDispatchIndirect.html)和[参数结构规范](https://docs.vulkan.org/refpages/latest/refpages/source/VkDispatchIndirectCommand.html)。

`bc build` 为有效域额外输出 `.dispatch.spv`。模块接口可以声明和传递前缀域能力，跨提交由相同数据的所有权承载，见下节；通用动态任务仍待完成；前缀集合运算见下文。

### 保留原索引的稀疏并行域
`where(dataset, dataset.bool_field)` 在原元素域中只执行 bool 字段为真的元素；第二个 dataset 可以是另一个只读数据集。它是 parallel 的域语法。例如：

```c
struct Path { bool active; float3 contribution; };
struct Pixel { float3 radiance; };
void accumulate<const C:u32>(read dataset<Path,C> paths,
                               inout dataset<Pixel,C> pixels) on compute {
    parallel (pixel in where(pixels,paths.active)) {
        pixel.radiance += paths[index()].contribution;
    }
}
```

每个实例仍独占 pixels 的原位置，`index()`／`ordinal(pixel)` 是当前输入数据集中的原索引，不是筛选后的紧凑序号。未选择记录及填充不改写，`dataset.length` 仍为绑定容量。path 数值不授予任意散写，也不能反查此前显式 compact 已移走的记录；像素一一对应需要生产者一直保留共同源位置。

谓词为 bool 字段，可嵌套，例如 `flags.state.keep`；必须来自 read 数据集，或当前 inout 域自身。当前元素可以改写自己的活跃位，其他实例没有该位置的写权限；每个源阶段重新选择，不缓存旧活跃集合。其他 inout、write 或 atomic 数据不能充当谓词。谓词容量必须覆盖原域容量；已知静态形状在编译期检查，动态绑定由原执行入口检查实际容量。

可写 `parallel (item in where(prefix(items,count.value),flags.keep))`，prefix 仍需已有真实生产关系，仅在有效前缀内筛选。当前不开放多层 where、任意谓词表达式或把 where 直接作为集合算子的输入。独占写、普通索引、资源权限和阶段依赖保持原规则；向另一个可写目标 `other[index()]` 写入仍报 B166。

CPU 原生范围函数按原索引读取 bool，只对选中元素执行正文。GPU 用既有分层扫描稳定生成私有 u32 原索引序列，再按设备选中数量间接启动正文；原前缀的六层启动参数与选中正文的第七项分开，避免覆盖仍在读取的参数。计数、索引、摘要与参数由当前 Commands／Execution／Completion 持有，不增加公开 Count、逆映射、资源服务或 CPU 计数读回。

只有域、prefix 和 selection 相同，且通过原依赖分析的 map 才可能融合。谓词读进入阶段读集合，自身谓词的读写冲突保留必要边界。CPU 直线及统一控制流稀疏正文使用下述四元素 SSE2 计划，按原顺序收集四个选中索引，原索引的读取与独占写入逐 lane 寻址；未选中元素不执行正文，余下至多三个选中元素由同一原生范围函数完成。逐元素分歧及其他尚无批次表示的正文仍按原标量指令执行。GPU subgroup 按实际执行参与者协作，不授予额外写权限。稀疏 tile 的邻接与发布尚未定义，含 tile 的 where 阶段报 B250。`bc build` 额外导出 selection、tail、add、scatter 和 dispatch SPIR-V，DLL 携带同一选择域的最终代码与执行布局。

### 模块接口中的有效域

`.h` 与 `.b` 都声明前置条件和交付关系，顺序不影响接口匹配。下列模块可独立检查；消费函数不依赖生产模块的私有实现：

```c
// producer.h / producer.b 的签名一致
void select(read dataset<Item> input,
               write dataset<Item> output,
               write dataset<Count, 1> count) on compute
    ensures prefix(output, count.value) {
    compact(input, input.keep, output, count.value);
}

// consumer.h / consumer.b 的签名一致
void consume(inout dataset<Item> items,
                read dataset<Count, 1> size, u32 delta) on compute
    requires prefix(items, size.value)
    ensures prefix(items, size.value) {
    parallel (item in prefix(items, size.value)) {
        item.id += delta;
    }
}

// 上层 .b 导入上述模块，.h 声明相同签名
void run(read dataset<Item> input, inout dataset<Item> output,
            inout dataset<Count, 1> count) on compute
    ensures prefix(output, count.value) {
    select(input, output, count);
    consume(output, count, 100);
}
```

`requires` 表示调用者必须提供的关系；`ensures` 表示当前实现必须证明的关系，不是可任意声明的承诺。Count 必须是独立、形状为 1 且只含一个 u32 字段的数据集；requires 的计数参数必须可读。缺少调用条件报 B191，无效关系声明报 B192，无法证明交付报 B193，`.h/.b` 约束不同报 B028。普通值函数不接受此类约束。

每个 计算函数 只在自己的 requires 下检查。调用时，编译器先满足被调用者的条件，再按声明的 write/inout 参数使调用方旧计数关系失效，最后接收被调用者已证明的 ensures；不从私有实现推断额外承诺。read 计数不能被模块改写，因此调用方已有关系可继续保留；函数直接改写计数后，必须重新生产对应关系才能交付。访问权限仍独立生效，write 参数不会因存在前缀关系而变为 read。

编译结果将无前置条件的公开入口保存为 `ExecutableProgram`，条件模板保存为 `Compilation.templates` 中的 `TemplateProgram`。两个类型都只能由编译器构造，只读访问已检查 IR。`CpuProgram::new`只接收前者，`CpuTemplate::new`接收后者；GPU先由`Compiler::spirv`或`spirv::compile_program`形成对应类型的映像，设备创建借用其`ExecutionPlan`，再用`Device::load`／`Device::load_template`装入普通入口／条件模板。条件执行接口必须取得实际存储持有的域关系。`bc check/build` 可以检查与导出条件模板的原生代码，通用 `compute` 文件入口不能直接绑定裸 Count 执行条件模板。跨模块组合仍可在一个闭合入口中执行，不为普通模块调用读回计数或新增一次提交。

`CpuProgram::program()` 与 `GpuProgram::program()` 只读借用实际执行程序的布局；原生消费者据此创建 dataset、读取字段偏移和步长，不为长期执行另保留一份 IR／字段布局镜像。

### 跨执行与跨提交的域所有权

`DomainBindings<Dataset>`／`DomainBindings<GpuDataset>` 移动实际存储及其已生产的关系。关系由成功执行的编译程序交付，不能从任意计数、原始文件或相同数值重新构造；没有全局计数注册表、哈希或版本票据。`datasets()` 共享借用同一原`Vec<D>`，可读取原槽及其len／capacity；`into_datasets()` 取出存储并放弃关系，宿主随后可自行修改。

- CPU 闭合入口使用 `CpuProgram::run_owned` 在成功时返回带关系的绑定；`CpuTemplate::run` 只接收这类绑定，失败输入交接见下述拥有型接口。既有 `CpuProgram::run(&mut [Dataset], ...)` 继续执行普通程序，它不交付可持续持有的域关系。
- `rebind(order, additional)` 按旧绑定和追加绑定的联合顺序选取并重排所有权，同时重排关系中的数据／计数索引。未一起保留的数据与计数不再构成关系，同一存储不能重复绑定为两个可写所有者。追加的普通数据没有域关系。
- 每次执行后，按接口声明的 write/inout 计数权限使旧关系失效，再接收经过编译证明的 ensures；read 计数保留调用方原关系，即使被调用模块没有重复声明 ensures。前端模块调用和宿主分次执行使用相同权限规则。
- `Device::upload_domains`／`download_domains` 移动实际传输后的存储及关系。上传／下载仍是显式物理传输，不把 CPU 与显存伪装成同一存储。普通值记录可直接往返消费；CPU／GPU view 分别通过 `with_views`／`upload_with_views` 绑定真实不可变资源。物理地址复制不转移引用图，也不能替代引用所有权。

GPU 的条件绑定来自 `Completed::into_bindings`，或直接来自尚未等待的 `Submission`。后者由 `GpuProgram::submit_after`／`GpuTemplate::submit_after` 接收。以上节 select／consume 的参数顺序为例，producer 是已编译的 select，consumer 是已编译的 consume，input 是预先上传的 source／output／count 三项：

```rust
let pending = producer.submit(input, &[])?;
// 保留上一项的 output 和 count；原 source 不成为消费模块的参数。
let pending = consumer.submit_after(
    pending, &[1, 2], Vec::<blcompute::vulkan::GpuDataset>::new(),
    &[blcompute::Value::U32(100)],
)?;
let persistent = pending.wait().map_err(|(error, _)| error)?.into_bindings();
```

`submit_after` 消费前项提交和数据，不先等待它。GPU 直接读取前项 Count 并生成间接启动参数，CPU 不读回计数。跨提交只支持同一设备队列；原生宿主绑定边界检查实际存储与接口条件，类型、访问、条件证明和模块实现继续在编译期检查，不把动态 Rust 绑定顺序误称为静态证明。

每份 `GpuDataset` 的唯一存储持有最后一次实际使用它的执行上下文。分支共享原生完成事实和命令资源，分别拥有独立数据，不再维护另一张存储来源表。继续提交前，执行上下文按同队列成功提交次序归并依赖，在分配新临时页之前用已有 Fence 的 [vkGetFenceStatus](https://docs.vulkan.org/refpages/latest/refpages/source/vkGetFenceStatus.html) 观察完成前缀；完成的命令、pipeline 引用、描述符、临时页和已丢弃存储立即释放。没有新增计数读回、等待线程、全局票据或定时轮询。仍有分支持有的数据及不可变 view 的真实引用继续存活，已经观察完成的命令资源不随这些数据长期保留。

回收前仍先记录同队列 RAW／WAW／WAR 依赖：Fence 完成观察不替代设备内存可见性，依据 [Vulkan 跨提交屏障](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdPipelineBarrier.html)。成功观察到 signaled 后复用原有完成结果，资源析构不会再次调用原生等待。显式 `wait()` 只等待当前末项 Fence；它的作用域包括同队列此前命令，因此前序资源可直接释放，依据 [Vulkan Fence 作用域](https://docs.vulkan.org/spec/latest/chapters/synchronization.html#synchronization-fences-signaling)。丢弃尚未完成的数据，只把它的物理存储移交给最后使用点，已有兄弟分支可继续推进；最后一个执行所有者退出时才结束仍未完成的真实消费者。原生完成后允许销毁命令池，见 [vkDestroyCommandPool](https://docs.vulkan.org/refpages/latest/refpages/source/vkDestroyCommandPool.html)。

这没有新增 `prefix<T>` 存储容器，也不改变 GPU 数据页表 ABI。提交序列仍按宿主确定；已完成的前序资源在继续提交、显式完成观察或末项等待时回收，当前未完成工作和实际存储消费者继续持有必要资源。CPU 长期排入快于 GPU 完成的真实任务时，未完成工作仍会增加，尚无自动背压。多生产者与待完成结果分叉见下节；CPU／GPU 只读 Count 共享见下节；多队列／多 GPU 和设备自主生成任务仍未完成。新缓冲区上传与GPU清零现已返回带真实完成点的存储，后续提交可以直接消费；纹理上传仍显式同步，不能将缓冲区能力外推为所有资源均异步。

### 多生产者汇合与待完成结果分叉

`DomainBindings::join(parts)` 消费多个绑定组，依次连接它们的存储槽，并平移各自域关系中的数据／计数索引。每个 dataset 和 Count 保持自己的范围，记录内容与实际数组不复制；记录串接、归约或其他业务合并由 `.b` 程序定义。CPU 执行结果、GPU Completed 的绑定，以及显式传输后的绑定都可如此组合，再用 `rebind` 对应下游接口。

`partition(groups)` 消费一个绑定组，按每组索引转移所有权；未选择的存储沿实际最后使用点释放。它与 rebind 共用同一重排规则：唯一可写存储不能重复，已冻结的 CPU／GPU 存储可以产生同一所有者的只读绑定。空分组有效。每组数据与实际 Count 关系一起交付；没有对应 Count 或其只读别名时放弃关系，随后 join 不会根据相同计数或记录内容恢复关系。一个 Count 不能被复制为多个可写所有者。裸 Dataset 移入 CPU 绑定时保持独占所有权，冻结后的 CpuDataset 可共享只读存储。

`DomainBindings<GpuDataset>::freeze(&[count_binding])` 消费绑定组，在保留现有域关系的同时冻结所选物理存储；此后这些槽只能交给 read 参数。它可以直接消费 pending 结果，不读取 Count，不复制记录或申请 GPU 数据页。后续 partition／rebind 允许重复选择冻结槽，每个别名持有同一 ReadDataset 所有者及其真实生产依赖；分支新增的 GPU 使用仍由原 Execution 与最后使用点管理。

例如生产者绑定为 source／queue_a／queue_b／count，并已交付两个队列与 count 的有效域关系：

```rust
let bindings = pending.into_bindings().freeze(&[3])?;
let mut branches = bindings.partition(&[&[1, 3], &[2, 3]])?;
let b = branches.pop().unwrap();
let a = branches.pop().unwrap();
let a = advance_a.submit(a, &[])?;
let b = advance_b.submit(b, &[])?;
let joined = DomainBindings::join([b.into_bindings(), a.into_bindings()]);
// 保留 a、b 及任一相同冻结 Count 的绑定，域关系随真实来源重排。
let joined = joined.rebind(&[2, 0, 1], vec![])?;
let pending = combine.submit(joined, &[])?;
```

关系只认原编译器已交付的来源与计数字段。同一冻结 Count 的别名可在 join 后直接满足接口，也可在 rebind 中保留任一个别名；独立且数值相同的 Count、裸数据向量和已被修改的 Count 不能据此取得关系。冻结数据不能绑定 write／inout／atomic；未冻结的唯一存储仍不能重复取出。DomainStorage 由存储实现提供只读身份与绑定；通用绑定辅助函数可扩展，CPU 借用入口接受 Dataset，拥有型入口接受 CpuDataset，GPU 入口接受 GpuDataset，其他存储实现不能代替目标物理存储。不另加封闭类型列表；原 C ABI 与 B 类型／语法不变；CPU 共享绑定见下段。

CPU 的拥有型 `CpuProgram::run_owned`／`CpuTemplate::run` 返回 `cpu::OwnedRunResult`：成功为 `Ok(DomainBindings<CpuDataset>)`，失败为 `Err((error, Bindings<DomainBindings<CpuDataset>>))`。错误携带原 `Box<dyn Error + Send + Sync>` 和输入经既有 `Into` 转换后实际持有的同一组数据、域关系及全部纹理；移动这些所有者，不复制载荷，不承诺转换前不同 Rust 容器的头部地址。前置拒绝保留原内容与有效域；已取得写访问后的失败交回实际部分写入及原 Count 失效结果，只有全部阶段完成才产生新保证。调用者可继续持有失败输入，或显式结束它并传播原错误；没有自动重放。成功后纹理的本次同步消费结束，返回的数据继续持有其实际所有权。`CpuDataset::Owned(Dataset)` 持有独占数据，`CpuDataset::Read(ReadDataset)` 持有原 Arc 冻结存储；`Dataset`、`ReadDataset` 可直接转为相应绑定，普通 Dataset 向量仍可作为输入。返回绑定以同一 read 接口暴露 words／布局；`CpuDataset::words_mut()` 只允许独占存储并返回 Result，冻结存储不转成可写切片。旧 `CpuProgram::run(&mut [Dataset],...)` 和 C ABI 保持，复用同一阶段执行主体。

Release Rust SDK可由`CpuProgram::run_owned_observed`或`CpuTemplate::run_observed`将实际范围计时追加至调用者持有的`cpu::CpuRangeTimings`。每组记录保留实际执行计划阶段索引、当次装入入口地址及原生调用的半开范围。每个范围保留Windows线程标识、首尾QPC tick和入口返回的失败状态。`ticks_per_second`是QPC频率，线程用户态与内核态累计值以100ns为单位，查询失败则返回Windows错误码。CPU入口完成前由原工作者持有写入位置，原调用返回后由调用者读取记录及原结果，入口地址只描述当次装入。已经调用的失败范围仍有记录，前置拒绝或空执行没有调用记录。普通入口无需采样，等待及整次执行的耗时可在各自边界测量。

`DomainBindings<CpuDataset>::freeze(&[count_binding])` 保留原 payload、实际地址、子资源与域关系，原 `ReadDataset::binding()` 也可直接产生 CPU read 绑定。partition／join／rebind 的只读别名规则与 GPU 相同；独立 CPU 消费者可持有同一只读 Count，实际调用归还前数据与子资源一直由原所有者持有。额外独占输出需要转为 CpuDataset，例如 `owned.rebind(&[0,1,2],vec![output.into()])`。不以 Count 数值相同恢复关系，也不允许共享绑定进入 write／inout／atomic 参数。

显式 `Device::upload_domains` 接收 CPU 带域绑定，`download_domains` 返回 `DomainBindings<CpuDataset>`；在一次传输原有的局部映射内，根绑定和 view 子资源按同一实际冻结所有者共享目标，只读能力保持。多个 Count 别名不分别分配数据页／CPU 数组；内容相同的独立来源仍分别传输。CPU 地址与 GPU 页表地址由目标真实所有者生成，映射随操作结束释放，没有隐式迁移、跨操作缓存或统一地址空间。单个 `Device::download` 仍返回显式读回的新 Dataset，不交付原有效域；拥有型 CPU 结果继续保留实际关系。

CPU 的 `CpuProgram::run_owned`／`CpuTemplate::run` 成功结果可直接分区。GPU 的 `Submission::partition(groups)` 可在生产者仍 pending 时直接分叉；`Submission::into_bindings()` 同样不等待，返回的绑定可 join、partition 或传给下一次 submit。Completed 的绑定也使用同一分区实现。分区只转移独立数据及其执行寿命，不下载数据、不复制记录、不申请 GPU 数据页。

例如一个生产者依次绑定 source_a／temporary_a／count_a／output_a／source_b／temporary_b／count_b／output_b，交付两个独立有效域；advance 的参数为 queue／count，combine 为 a／ca／b／cb／total：

```rust
let pending = producer.submit(inputs, &[])?;
let mut branches = pending.partition(&[&[3, 2], &[7, 6]])?;
let right = branches.pop().unwrap();
let left = branches.pop().unwrap();
let left = advance.submit(left, &[blcompute::Value::U32(1)])?;
let right = advance.submit(right, &[blcompute::Value::U32(2)])?;
let pending = combine.submit_join(
    [right, left], &[2, 3, 0, 1, 4], vec![total],
    &[blcompute::Value::U32(3), blcompute::Value::U32(5)],
)?;
let results = pending.wait().map_err(|(error, _)| error)?.into_bindings();
```

这些接口消费原所有者，Rust 不允许分叉后继续使用原 Submission 或重复移动同一分支。多份独立输出各自拥有 Count；它们可独立计算再汇合，但不能把同一可写存储或 Count 复制给两个分支。丢弃一个 pending 分支时，它的存储延迟至最后使用点完成才释放，其他分支不必为此等待。延后使用的分支保留自己的数据，即使生产者的命令和临时页已经释放。

本节示例用map_err显式结束失败提交并返回原错误；需要继续持有输入的调用者应匹配Err((error, submission))。执行资源与时间戳的消费者不同：`is_complete()` 可释放已完成命令、临时页及 Fence；显式采集的原Submission仍保留查询池，之后`wait()`可读取正确的`Some(GpuTimings)`，不再次进行原生等待。`GpuProgram`和`GpuTemplate`默认不采集时间戳；`with_timestamps(true)?`请求后续提交采集，`with_timestamps(false)?`关闭。`Completed.timings`、`Completed::into_parts()`第三项及`GpuCompletion::wait()`结果中的计时为`Option<GpuTimings>`；没有采集或没有执行阶段返回`None`，真实查询得到零仍为`Some`。开启时若当前计算队列不支持时间戳，直接返回错误；普通计算不要求计时能力。消费 Submission 做分区或后续提交时，不再保留无人读取的旧时间戳；查询池仍由实际 GPU 命令持有到完成。

`GpuProgram::submit_join`／`GpuTemplate::submit_join` 消费同队列的多个未等待提交。order 索引按照传入生产者的数据槽依次连接，再接追加数据；接口参数次序可独立于原生提交次序。假设两个 select 的绑定依次为 source／queue／count，消费接口依次为 a／ca／b／cb／total 和两个增量，且声明两份 requires：

```rust
let left = producer.submit(input_a, &[])?;
let right = producer.submit(input_b, &[])?;
// 逻辑输入顺序 [right, left]，下游参数仍按 [a, ca, b, cb, total]。
let pending = consumer.submit_join(
    [right, left], &[4, 5, 1, 2, 6], vec![total],
    &[blcompute::Value::U32(3), blcompute::Value::U32(5)],
)?;
let results = pending.wait().map_err(|(error, _)| error)?.into_bindings();
```

每个生产者的 Count 继续在 GPU 上生成和消费，汇合不先等待生产者或读回计数。原生队列所有者只在成功 vkQueueSubmit 后记录私有 u64 提交顺序；执行资源按该顺序归并回收，Fence 仍是完成事实源。该序号不进入源语言数据、不签发域能力，也不替代 Count。未继续绑定的物理存储交回它实际的最后使用点，不依赖传入生产者的逻辑位置。交接失败时，存储也沿相同寿命释放；仍有兄弟分支持有该执行时，失败分支不阻塞它。

`Bindings<D>` 的 datasets 可为普通 GPU 数据向量或 `DomainBindings<GpuDataset>`。所有执行最终使用同一带域绑定表示；普通向量没有已生产关系。追加预先上传的 CPU 结果或已完成 GPU 结果时，直接传入带域绑定，既有关系和待汇合 GPU 结果一起重排。需要加速结构时，使用相同 `Bindings { datasets, accelerations, textures }`。`submit_after` 是一个前项的汇合入口，执行与生命周期实现共用。

缓冲区上传仍是显式传输，但不在API内等待复制完成；CPU上传结果与尚未等待的GPU结果可以直接汇合，沿真实最后使用点建立原队列依赖。下载等待实际结果，纹理上传仍同步；没有隐式统一物理内存或新GPU调度语言。源语言组合的条件继续由前端检查，动态 Rust 路由在实际绑定边界检查归属、唯一所有权与已有关系。

### 有效前缀上的集合运算

同一 `prefix` 域直接用于数值 sum／min／max、scan 和 compact，不通过读取尾部数据猜测有效工作量：

```c
struct WorkItem { u32 units; f32 energy; bool keep; };
struct WorkSum { f32 value; };
void process_queue(read dataset<WorkItem> queue,
                      read dataset<Count, 1> count,
                      write dataset<Count> offsets,
                      write dataset<WorkSum, 1> total,
                      write dataset<WorkItem> next,
                      write dataset<Count, 1> next_count) on compute
    requires prefix(queue, count.value)
    ensures prefix(offsets, count.value)
    ensures prefix(next, next_count.value) {
    scan_exclusive_sum(prefix(queue, count.value).units, offsets.value);
    reduce_sum(prefix(queue, count.value).energy, total.value);
    compact(prefix(queue, count.value), queue.keep, next, next_count.value);
}
```

三个运算只消费前 count 条记录。扫描保持该有效长度，前端为 offsets 建立同一 Count 的关系，之后可直接使用 `parallel (v in prefix(offsets,count.value))`；筛选为 next 生产独立的新计数；本例 sum 写一个 f32 结果，其他数值归约可写同类型标量或向量字段。空有效域的归约输出对应单位元、compact 计数为 0，scan 不写记录。输出有效域之后的原内容保留，带有非零值、其他记录或无效数值的尾部不会进入运算。

扫描输出容量至少覆盖输入绑定容量，compact 继续要求最坏情况下可容纳整个输入绑定容量，谓词容量仍与输入相等；不会用某一次选择率推导安全分配。输入有效计数不能与本运算的输出别名。访问、类型、形状与域证明仍由同一前端处理；源域计数进入阶段读集合和跨元素依赖。条件模块可独立检查与导出，再由上层入口满足 requires 后组合执行。

CPU 按有效数量执行已有范围集合代码。GPU 的参数生成阶段读取当前 Count，一次写出每层的 X/Y/Z 和有效数量；递归归约、局部扫描、前缀传播和 scatter 分别使用对应层级的间接参数，全程不为调度读回计数。集合中间页与间接参数页分别占据必要的内部绑定槽，布局索引由同一 IR 提供；源程序参数与原数据页表 ABI 保持一致，提交拥有全部资源直到完成。

多维启动补齐的整组在进入共享存储屏障前统一退出；有效尾组的所有线程继续参加屏障，域外值贡献单位元。该控制流遵守 [SPIR-V 工作组屏障规则](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html)。命令的递归层数与临时存储目前仍按绑定容量预留，各层启动规模使用实际有效数量；这不是设备生成任意新任务，也未取消容量、显存或 u32 记录域限制。

### 原生稳定键排序

`stable_sort_by_key(input.key, output)` 按 u32 键升序重排完整记录；相同键保持输入顺序。源记录保持原值，输出只写实际输入域，容量尾部保留原内容。有效前缀写成 `stable_sort_by_key(prefix(input,count.value).key, output)`，输出继续持有同一 Count 的有效域关系：

```c
struct SortItem { u32 key; u32 id; };
void order(read dataset<SortItem> input,
              write dataset<SortItem> output) on compute {
    stable_sort_by_key(input.key, output);
}
void order_queue(read dataset<SortItem> input,
                    read dataset<Count, 1> count,
                    write dataset<SortItem> output) on compute
    requires prefix(input, count.value)
    ensures prefix(output, count.value) {
    stable_sort_by_key(prefix(input, count.value).key, output);
}
```

`.h` 声明相同接口，函数体位于 `.b`。输入须在当前作用域内已初始化并具备读取能力，排序只读消费；输出为独立 write/inout dataset，记录类型相同；输出容量至少覆盖输入绑定容量。输出不得与自身有效计数别名。字段类型、访问／别名、静态容量、结果域关系分别复用 B175、B170、B188／B189、B191／B193；动态实际容量继续由现有设备绑定边界负责。

键支持u8／i8／u16／i16／u32／i32／u64／i64／f32／f64数值标量、现有数值向量及嵌套字段，也可在输出参数之前列出多个键，例如 `stable_sort_by_key(input.space,input.high,input.low,output)`。按参数从左至右的优先级做字典序排序，默认升序，向量按 x、y、z、w 展开；较高优先级相同才比较下一个分量，全部键相同才保持输入顺序。所有键必须来自同一 read dataset 和同一有效域；有效前缀的每个键都要写相同的 `prefix(input,count.value)`。bool、记录、数组、矩阵和 view 键仍报 B175。`uint2` 是 x 优先的数值向量，不隐含低位／高位语义；原生64位整数可直接使用u64／i64字段，不要求用户拆分高／低字段。自定义比较器及含view的可写输出仍未实现。

在排序键参数上使用 `descending(field)` 指定降序，例如 `stable_sort_by_key(descending(input.priority),input.id,output)` 按 priority 降序、id 升序。`descending(input.vector)` 的全部向量分量均按降序参与原字典序优先级，嵌套字段和 `descending(prefix(input,count.value).key)` 同样支持。方向属于每个静态键，不改变域、原记录或相等完整键的稳定性。`descending` 只在此排序键位置接收一个字段参数，参数数目错误报 B169；它不是普通值函数，不接受比较器、动态方向或计算表达式作为键。

键的顺序由源类型确定：

| 键 | 升序定义 |
|---|---|
| u8／u16／u32／u64 | 对应位宽的无符号数值顺序 |
| i8／i16／i32／i64 | 对应位宽的有符号数值顺序，最小值在前 |
| f32／f64 | 对应位宽全部编码的全序；负 NaN、−Infinity、负数、负次正规数、−0.0、+0.0、正次正规数、正数、+Infinity、正 NaN |

f32／f64分别使用与[Rust f32::total_cmp](https://doc.rust-lang.org/std/primitive.f32.html#method.total_cmp)／[f64::total_cmp](https://doc.rust-lang.org/std/primitive.f64.html#method.total_cmp)一致的编码全序：正 NaN 的 signaling 编码在 quiet 编码之前，负 NaN 反向；同符号 NaN 的载荷也参与顺序。−0.0 与 +0.0 不是相同键，全部 NaN 也不会合为一个键。只有键编码完全相同的记录才按输入顺序保持稳定；整数同类型同数值也恰好同编码。这里是排序语义，不改变普通浮点 `==`／`<` 运算的含义。

装入当前键时，窄整数只读取其真实1／2字节，符号映射直接消费原Scalar.order_bias；有符号整数按实际位宽翻转符号位。f32／f64将负编码逐位取反、非负编码翻转符号位，降序反转完整全序编码，相等编码仍按输入顺序稳定排列。IR直接保存原字段字节位置、Scalar及方向，每项消费该全序编码的低32位；64位字段由高／低项共同排序，f64低字按同一字段高字的符号位决定是否取反，GPU沿一次选页取得同一字段的两字。读取和扩展使用原类型化存储入口。CPU和GPU都只执行整数位操作，不把f64数值转为f32，不对排序键做浮点算术／比较，也不增加float-preserve请求；数据类型及存储能力仍按原Vulkan声明和查询处理。原字段、NaN载荷、有符号零、次正规数及记录填充完整保留。

64位字段的高字优先于低字；整数高字复用原u32／i32次序，f64高字复用原f32整数编码映射，低字保留原Scalar的完整数值编码关系。两项继承同一方向，共用原键／原始序号暂存和最终一次gather，不增加键表、记录副本或另一个排序器。

两个后端都从最低优先级的32位子键开始，每个子键以 4 bit、16 桶进行八轮稳定基数分配；一个64位整数因此由两个32位子键处理。下一个分量直接按已有原始序号从源记录装入同一键槽；不保存全部键的副本，不为每个键生成一个完整记录中间集。中间始终只移动 8 字节的当前键／原始序号，最后 gather 一次完整记录，保留字段与 ABI 填充字节。CPU gather按实际尾部8／4／2／1字节复制，GPU gather复用原类型化物理存储复制完整记录，按Ty实际1／2／4字节存储单元与对齐交接；不宽读或覆盖下一条记录，页边界仍不拆记录。复制包含所有 ABI 填充，不对 bool、NaN、次正规数或有符号零做数值转换；对齐与物理指针访问遵循 [SPIR-V 规范](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html)。CPU 由自研 x64 内核完成键装入、各范围私有直方图、稳定散写与 gather；执行框架只为少量范围／桶计算前缀，继续复用 Windows 范围线程池，不调用另一套排序库。CPU两块键／序号工作区只预留可写容量，首次读取分别在原生初始化或scatter完整覆盖并成功完成之后，不预填无消费者的零值；直方图计数继续使用已初始化切片和每轮零基线。临时内存由当前执行拥有，所有范围实际结束后释放，分配或原生执行失败直接报告。

GPU 每个 64 线程组产生 16 桶直方图，桶优先、组序号其次排列；对该扁平序列做独占扫描即可同时得到桶起点与此前各组的数量。ballot 求出本 subgroup 内的相等键掩码和稳定序号，加上此前 subgroup 的桶计数后得到组内序号；全局前缀与组内序号共同给出唯一写入位置。B发射直方图与散射，活跃lane的键及原序号通过SSA合流交接。直方图只读取键，散射的键与序号在源和目标各共用一次选页。直方图扫描复用已有设备扫描层级，全部中间数据留在 GPU，提交持有页与 pipeline 到最后一个消费者完成。

Vulkan 不保证 LocalInvocationIndex 与 subgroup lane 的直接对应关系，因此排序内核明确申请 32 lane、full subgroups，并用 `SubgroupId * 32 + SubgroupLocalInvocationId` 合成逻辑输入序号。尾组仍完整参加 ballot／共享存储屏障；域外元素不贡献计数。依据为 [Khronos subgroup 序号规则](https://docs.vulkan.org/refpages/latest/refpages/source/SubgroupLocalInvocationId.html)与[完整 subgroup 能力定义](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceSubgroupSizeControlFeatures.html)。32 是当前排序后端的实现要求，不改变语言元素域，也不是所有 GPU 算法的固定 wave 宽度。

`Device::for_program`／`with_presentation` 按程序需求查询并启用 `VK_EXT_subgroup_size_control`、compute ballot、subgroupSizeControl 和 computeFullSubgroups；仅排序的 histogram／scatter pipeline 请求 requiredSubgroupSize=32 及 REQUIRE_FULL_SUBGROUPS。普通计算和光线查询不因此申请排序能力。没有声明能力的设备不能编译排序程序，缺少硬件能力明确报错；CPU 排序不依赖 Vulkan 设备。

有效域排序一次生成两组间接参数：记录域及其 `16 * max(ceil(N/64),1)` 直方图域，各自具有当前 u32 范围所需的六层参数。八轮的初始化、直方图、扫描、scatter 和最终 gather 均直接消费设备计数；空结果仍生成正确的零直方图。中间存储和命令层级继续按容量预留，没有 CPU 中途读回计数，也未实现 GPU 动态创建任意命令图。

`bc build` 导出初始化 `.spv/.x64`、各 shift 的 `.histogram.spv`／`.scatter.spv`、扫描 `.scan/.tail/.add.spv`、最终 `.gather.spv`，以及 CPU histogram／scatter／gather `.x64`；有效域另有 `.dispatch.spv`。自研 SPIR-V 生成 API 返回 Result，不适用于排序的后端 pass 明确返回错误。运行时仍直接消费编译结果，不以这些调试导出文件建立另一套执行状态。

排序及提交的具体规模、数值、性能与设备观测只在双台账记录；现有算子支持不表示达到成熟排序库吞吐或改善产品帧率。

## 值类型与顺序控制

支持 `f32`／`u32`／`i32`／`u64`／`i64`／`bool`，`float2/3/4`、`uint2/3/4`、`int2/3/4`，嵌套结构体、向量构造、固定 f32 矩阵和 swizzle。数值向量的value[index]按x／y／z／w逻辑顺序选择标量分量；u32下标须由常量、有界循环或现有范围关系证明小于分量数。可变局部、当前独占dataset字段和write／inout普通位置可写所选分量；重排swizzle及普通CPU位置借用保持真实分量地址，不要求连续布局、不复制载荷。基表达式及下标各按源码顺序求值一次；只读、原子、未初始化及实际越界沿原权限／范围规则处理，没有运行时范围检查。CPU与SPIR-V复用既有选择及索引IR，矩阵与记录仍通过其字段访问。支持局部变量、赋值、if/else、while、范围 for、只读数据集游标、break／continue、短路布尔表达式、数值转换和常见标量／向量数学。

数学包括 dot、cross、length、normalize、abs、sqrt、floor、sin、cos、exp、log、exp2、log2、pow、min、max、clamp。数学定义域不会被隐式修正；normalize 零向量、非法 log 等仍是数值设计问题。

`bitcast_u32(value)` 将 f32／float2／float3／float4 的每个原始 32 位分量解释为 u32／uint2／uint3／uint4；`bitcast_f32(value)` 做相反解释。它们各接收一个参数，保持分量数及全部位，不执行数值转换。正负零、非正规数、无穷及 NaN 载荷直接交接；之后的浮点算术仍遵守其本身语义，不承诺保留 NaN 载荷。bool、i32、矩阵、数组和记录不能作为这两个内建的输入，参数数量错误报 B153，类型错误报 B154。

例如 `bitcast_u32(1.0)` 得到 `0x3f800000`，`u32(1.0)` 仍得到 1；`bitcast_f32(0x80000000)` 是负零。已知输入在编译期间保留原位形成常量；GPU 对 IR f32 Constant 同样先保留 u32 常量字，再以 OpBitcast 交付 f32，保持特殊 NaN 等原始位。运行期进入同一 Unary IR 的 Bitcast，CPU 标量与 SSE2 复制原位，GPU 使用 SPIR-V OpBitcast。动态bitcast_f32／bitcast_u32及bitcast_f64／bitcast_u64将原允许位集合按目标的真实数值次序重新形成区间。原浮点有序编码与原始位在实际符号位断点两侧分别单调，区间只在该断点切分并映射端点，不数值转换、不枚举输入位模式。源数值相等、条件、非零标志、资源索引及借用身份不跨类型复制；目标非零或有界性只能由重新形成的数值集合及原消费者取得。正负零保持两种位模式，NaN／无穷及其载荷不被删除，未知位集合仍不能取得有限浮点许可。结果只使用原Proof.ranges，字段／向量、共同元素及公开模块输出按原合同交接；不保存来源图、运行表或额外拥有者，不改变原UnaryOp::Bitcast、机器指令、数据布局、对象与执行映像格式或C ABI。结果作为安全下标或除数仍须由原消费者证明自身关系。

`for (node in nodes)` 是每个并行实例自己的有界索引访问，不额外生成一批并行任务。`ordinal(node)` 读取游标索引；`seek(node, node.escape)` 将最内层游标移到指定位置并重新进入循环检查，服务线性 BVH 跳过子树。循环的索引受数据集长度约束；程序仍有责任保证跳转推进和算法终止。

普通数值 `for (d in first..last)` 在进入时各求值一次起止表达式，正文中的 d 小于已捕获的 last。当前循环正文及其 if 分支中没有 seek 时，范围事实也保留已捕获 first 的数值下界；例如 `for (d in 2..8)` 内可用 d 作整数除数，动态起点已有的非零下界同样有效。普通自增与 continue 保持该下界，改写生产 first 的局部变量不改变已捕获起点。嵌套 for／while 拥有自己的循环控制，不撤回外层起点事实。当前循环存在 seek 时仍按任意 u32 重入处理，不把初始下界施加到跳转后的位置；seek 的一般范围不变式尚未推导，循环终止仍由算法负责。

安全整数除法接受编译期非零常量；动态u32／i32／u64／i64标量及现有32位向量除数需逐分量证明当前值非零，标量操作数可广播；函数接口可沿既有requires nonzero合同交接。普通指针、任意地址散写、浮点原子、非原子任意共享写／动态屏障、GPU普通函数递归、动态任务和完整泛型仍未实现；GPU64位tile的已实现范围与当前原生缺陷见块内原子章节；普通CPU递归及开放泛型的当前范围按相应章节交付。

无小数点的正整数字面量当前为 u32，负整数字面量为 i32；带小数点的数值为 f32。整数按实际32或64位运算，溢出取对应低位；`0x` 十六进制中的 e/E/f 是数字，不作为浮点标记，紧随其后的加减仍是独立运算符；有符号最小值除以 -1 直接降低为取负，余数为 0，CPU 不执行会触发异常的 idiv。动态浮点到整数的转换必须满足目标范围；当前编译器不能完整证明所有数值定义域。

f32 的取负翻转符号位，`abs` 清除符号位，其余31位保持；标量和向量按各分量处理，正负零、次正规值及 qNaN／sNaN 载荷均按这一规则交接。CPU 标量／SSE2 与 GPU 统一使用对应的 XOR／AND 位操作，GPU 沿原 u32 字边界解释为 f32。普通程序中单独的取负／abs 不要求浮点算术保真能力；组合真实浮点运算、比较、数值转换或集合时，仍按相应操作的原能力与浮点语义执行。

## 整数常量表达式与不可变绑定

整数常量的加减乘、除余、位运算、移位和比较在前端按实际类型求值，直接生成既有 Constant IR。u32／i32保留低32位，u64／i64保留低64位，移位量分别取低五位／低六位，有符号右移保持符号；整数除法向零截断，MIN／−1的同位宽模商与零余数保持。向量按分量求值，标量广播、构造与 swizzle 使用原来的类型布局。f32常量四则复用模块const的逐操作舍入规则，f64常量的普通算术见f64章节；一般浮点数学函数、通用编译期解释器和函数循环求值仍未实现。

类型先于常量求值确定。数值字面量及尚未经过类型绑定的整数字面量表达式可以接受另一侧的数值类型；两侧均为未绑定字面量时，含 f32 使用 f32，否则含负整数使用 i32，其余使用 u32。局部绑定、参数、函数返回值和显式类型构造固定类型，已知数值不再赋予它们隐式转换权限。比如 `i32_value + 1` 使用 i32，而 `i32_value + u32(1)` 报 B148；bool 与数值比较需要显式转换。函数实际参数恰好为常量也不改变其声明类型、有符号比较或右移语义。

未被赋值的 let 直接引用原来的 Value／SSA 寄存器，保留常量、范围、域与类型信息，不再先写入局部槽再恢复。可变名称只从当前函数体派生，内联进入／退出函数时与其环境一起切换；无关函数的同名变量不会改变当前绑定的可变性。旧 local_ranges 与全局寄存器 ranges 表均已删除，数值区间随实际 Value／Proof 的分量传递。存在赋值的局部继续使用可变槽和既有分支／循环分析，不因某次执行没有进入赋值分支而冒充常量。

```c
let divisor = (1 << 3) - 1;
cell.value = cell.value / divisor;
let values = tile_publish(cell.value, (1 << 4) + 1);
u32 total = tile_reduce_sum(values);
```

安全整数除数及 tile 宽度消费同一份编译期结果；宽度仍必须是非零 u32，同阶段必须一致，设备资源限制仍由原有边界负责。无需另一套 AST 求值器或运行期计算宽度。

已知左值的 &&／|| 只生成实际需要的右侧代码，固定条件的 ?: 只生成选中侧代码并交付其值；所有操作数和分支仍先接受完整类型与能力检查。动态 ?: 的条件求值及优化见前文“条件求值选择式”。`1 / (8 - 8)`、`false && (x / 0 == 0)` 和未选表达式中的非法除法仍报 B152，不因折叠而隐藏源码问题。动态选择和可变除数不被伪装成编译期常量；动态 u32 除数沿用前文的非零证明，未支持的类型仍明确拒绝。

无消费者的 Map 指令全部消除后，编译器一并删除该阶段，source_stages 仍记录优化前展开数量；源程序为空或只有静态空范围时，两者都可为零。归约等集合阶段继续保留空输入下的单位元／计数产出，不按逐元素指令是否为空删除。

没有执行阶段时，CPU 不生成范围函数，GPU 不创建计算管线、布局、描述符或查询池。计时结果为`None`，表示没有采集到程序阶段跨度；前序上传、实际依赖交接、提交及等待仍有真实生命周期与耗时。Submission 继续沿既有原生 Fence 完成后交还数据，into_bindings／partition 继续由数据持有最后使用点，不能据零阶段计时提前释放资源。参数与有效域能力仍按原 ABI、requires／ensures 检查；无来源的 ensures prefix 仍报 B193，不能以空程序创建能力。

## 局部值转发与函数封装

普通值函数和 cooperative 函数内联后，参数、返回值与局部变量进入同一份结构化 IR。优化器记录局部槽当前已知的 SSA 值，后续读取直接使用这个值；既有后向活跃性随后删除没有真实读取者的局部写入及空槽。函数封装不必因此增加一层内存读写，也不建立额外的运行时对象或调度任务。

分支分别分析，只有两条路径交付同一个 SSA 值时才在合流后继续转发。循环入口先清除所有可能被条件或循环体改写的局部槽知识，循环中的读取与写入仍按实际顺序处理；循环携带的可变状态保留，进入前已知的值不会被误当作循环不变量。跨 tile 阶段的真实私有值继续由同一 IR 活跃性与阶段切分保存。

数值转换的公共表达式以目标标量类型和来源区分；相同来源转为 i32、u32、f32 或 bool 不能相互替代。此优化不重排浮点运算、不跨协作指令合并参与者，也不等于完整 SSA 局部提升、物理寄存器分配或跨元素 SIMD。某个封装是否完全消失，仍取决于实际源程序及控制流。

## ABI 与代码生成

零宽B值的本地调用范围没有标量成员或物理载荷。MemoryAlias::local_range只交接非空实际槽范围，空范围不参与槽重映射、干涉和物理栈组分配；原CallReference仍表达其类型、word／offset及访问关系。Windows x64原生调用需要零字节引用时，由当前函数／调用栈帧提供地址锚点，后续实际偏移仍沿同一入口解释，不要求不存在的PrivateSlot。原声明大小、对齐及借用继续有效；该地址只表示零字节范围，不产生一份载荷或资源。

| 项目 | 当前规则 |
|---|---|
| u32／i32／f32／bool | 大小4、对齐4 |
| u64／i64 | 大小8、对齐8 |
| float2 | 大小 8、对齐 8 |
| float3 | 大小 12、对齐 16；其后标量可占偏移 12 |
| float4 | 大小 16、对齐 16 |
| 结构体 | 字段按自身对齐排列，最终步长按最大字段对齐 |
| BLBody | position@0、velocity@16、energy@28、stride=32 |
| 小参数 | word0 为范围末端、word1 为分区起点，随后数据集长度与对齐后的值参数 |
| 编译小参数容量 | Compiler.max_parameter_bytes，默认128字节；可按本次目标显式配置 |

B103按上述布局及实际类型对齐报告完整字节数，并与本次Compiler的参数容量比较；该前端路径不查询设备。Rust调用方以with_max_parameter_bytes(bytes)配置，max_parameter_bytes()读取同一实例的值；Compiler clone共享原CPU许可预算并携带该配置，对clone设置新值不改变另一实例。CLI在命令前使用--max-parameter-bytes N，可与--jobs或--cpus组合；未指定时沿用128字节。字节数是本次编译选择，不是语言永久上限或硬件查询结果。

```powershell
bc --max-parameter-bytes 256 -c workload.b --module -o target/workload.obj
```

需要按真实GPU编译时，调用方先从既有Instance枚举并选定PhysicalDevice，使用其max_push_constant_bytes()返回的已查询属性；该句柄继续交给for_program／for_programs创建实际Device。编译器不依赖blcompute或另行选择设备：

```rust
let compiler = blanguage::Compiler::available()?
    .with_max_parameter_bytes(physical.max_push_constant_bytes());
let built = compiler.build_entry(
    path, root, entry, blanguage::ast::Target::Gpu, output,
)?;
```

配置只约束本次新生成的参数布局，包含检查、CPU应用内提取入口、独立模块和算法库；数据类型、对齐及CPU／GPU存储规则保持。原BLBI与COFF缓存将该真实编译配置计入输入身份，同配置可复用，配置变化重新按本次上限生成或拒绝，旧大容量产物不能让较小配置跳过检查。

产物的ExecutionPlan／ProgramInterface继续通过uniform_bytes记录实际布局要求，不记录或占满编译时允许的全部容量。独立对象／归档及应用按已编译映像交接该要求；链接不重读源码或重新选择编译配置。导入的既有程序保持自身实际要求，不因调用方新生成布局采用较小配置而改写。原Device::for_program(s)、PhysicalDevice显式程序入口及呈现程序入口在创建逻辑设备前，直接将实际uniform_bytes与候选已查询的maxPushConstantsSize匹配；多程序需求取最大值，不把各自独立的参数块相加。Device::new／physical.device没有声明程序，后续装入仍在原Device::load核对已有设备的真实容量。例如编译配置允许512字节、实际产物需要260字节、候选只有256字节时，程序创建入口在vkCreateDevice之前明确拒绝；已有基础设备的装入仍准确拒绝。该容量选择不改变其他数值／设备能力及资源完成规则，ABI编码身份保持原值。原生属性和命令范围见[Vulkan limits](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceLimits.html)及[vkCmdPushConstants](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdPushConstants.html)。

Rust 消费者从编译结果获取布局，不能手写另一份结构体步长。CPU 普通范围函数 ABI 为 `(buffer pointers, parameter words, end index, begin index)`；数值归约与普通扫描经第五个结果指针按类型实际字节偏移写出分量。第二个参数始终指向程序统一参数，扫描／压缩／分段归约的范围前缀与排序范围元数据通过指针表末尾的私有项传递；每个工作回调只持有自己的指针表，原记录、统一参数和摘要数据不复制。自定义传播把私有项作为仅原生降低可见的只读 Buffer，用已有 Load IR 读取，不新增源语言资源或 GPU binding。分段计数沿值摘要之后的标记／u32计数存储字交接，源程序不暴露这些执行参数。

SPIR-V 1.4 直接按标准编码。GPU 的逻辑数据集通过只读的 u64 设备地址页表访问最多 256 MiB 的物理页；记录不跨页，根记录stride及所有可能view的直接元素size均须容纳于一页。非空固定数组沿元素类型取得同一view要求，零长数组不引入字段要求，变体的所有可能载荷均参与；这项类型能力查询不要求展开数组各位置或生成字段偏移。源语言的元素索引仍是 u32。源语言u64普通整数与这里的物理地址分别表达；普通u64不自动授予原生指针或资源权限。分页、上传、读回和递归归约临时存储由同一执行框架拥有。当前普通 GPU map 每组默认 64 个线程，tile 阶段使用其源码宽度；这不是假定硬件 wave 必须为 64，也与 Burley SSS 的采样数无关。驱动仍负责把 SPIR-V 转成具体 GPU 指令；自研语言不等于绕开硬件驱动。

```powershell
.\target\release\bc.exe check ..\BLEngine\Programs\render\frame.b ..\BLEngine\Programs 'frame<2,3840,2160,2,8294400,33177600>'
.\target\release\bc.exe build ..\BLEngine\Programs\simulation\statistics.b Saved\Compiled ..\BLEngine\Programs
```

B 直接写出 SPIR-V 类型、常量、变量、资源绑定及配套装饰，沿用原着色器的 ID、绑定顺序和块布局。普通标量二元和单目运算、比较、数值转换及数学指令由 B 选择和发射。实际位宽、NaN 谓词及浮点禁止收缩沿原数值语义交接。build 输出 `.spv`、归约 `.tail.spv`、CPU `.x64`、数学入口 `.reloc` 和布局 `.layout`。引擎运行时直接使用同一编译器结果和后端，不靠这些调试导出文件再建立第二套权威表示。

### x64 物理寄存器与栈槽

逐元素范围内核不再给每个 IR 寄存器和局部变量固定保留一份栈存储。同一B着色模块消费原结构化IR和共享B循环活跃查询，建立SSA值与可变局部槽的干涉、复制亲和及跨调用关系，按实际引用与饱和循环权重着色；原Rust容器持有唯一图、位置和集合，IR与实际读写事实只在本次同步调用中借出，必要分叉集合随最后消费者释放；同时存活的值不能占用同一位置。分支、break／seek、循环回边和 tile 阶段私有值均参与分析。寄存器不足的值分配溢出槽，不重叠的寿命可以复用同一槽。下面的分配表描述标量降低；四元素 SIMD 的位置见下一节。

| 用途 | 当前分配 |
|---|---|
| 整数与 bool 值 | R8D、R9D、ESI、EDI、EBP |
| f32 值 | XMM2–15，各保存一个标量值 |
| 跨数学／C ABI 函数调用的值 | 使用非易失寄存器或溢出槽；不把 R8／R9、XMM2–5 当成可跨调用保留的存储 |
| 内核临时运算 | RAX／RCX／RDX、R10／R11、XMM0／1，独立于值分配 |
| 范围与绑定 | 既有 RBX、R12–15；全范围入口／退出保存恢复 |

实际使用到的非易失通用寄存器保存全部 64 位，XMM6–15 保存全部 128 位，每个范围调用仅在入口／退出各处理一次。调用区按实际数学／C ABI 调用的最大 shadow space、栈参数、按值副本与返回缓冲需求分配；没有调用就不生产调用区。栈帧保持 16 字节对齐；大帧在一次实际减栈前调用同映像的无帧叶辅助逐页触及，零字节局部帧无需调整 RSP。寄存器规则依据 [Windows x64 ABI](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention?view=msvc-170)。

普通应用、计算范围和集合辅助入口从实际序言指令交付同一Windows x64展开编码，记录压栈、帧分配及非易失GPR／XMM保存位置。PE／COFF消费同一代码的主函数末端与展开字节；独立CPU映像装入将本程序各范围／集合入口及命名callee的代码、展开字节和同一有序函数表放入共享Allocation的一块VM，完成重定位、页面权限和指令缓存刷新后调用RtlAddFunctionTable，成功后才交付入口。最后一个真实消费者结束后先RtlDeleteFunctionTable，再释放该VM及原DLL引用。无帧探栈叶辅助和字面量不属于主函数展开范围。源码行号调试、B异常处理语义及运行时栈耗尽诊断仍按各自未完成范围记录，原生展开不替代这些能力。

寄存器间的 f32 值使用完整 XMM 位复制，避免为未使用的高位保留依赖；逻辑运算仍只消费各标量低 32 位。tile 保存／恢复直接消费分配位置与既有逐 lane 私有流，不再要求中间值同时拥有永久栈位置。算术次序保持，数值转换与数学定义域不因此改变。

标量和四元素 SIMD 共用活跃性与着色实现，基础指令直接消费各自的分配位置；活跃区间拆分和寄存器合并继续开发。单个标量驻留 XMM 与跨元素 SIMD 是两件事。GPU 继续由同一 SPIR-V 后端与驱动分配物理寄存器，没有复制一套 CPU 分配结果到 GPU。完整集合库、非原子任意共享写／动态协作和多设备执行仍未完成，性能取决于数据组织、工作量与具体代码。

### x64 直接操作数

标量算术、比较、局部复制、常量、参数与数据加载／保存直接使用已分配的 GPR、XMM 或溢出槽。目标与来源相同的位复制不生成指令，栈到栈复制按硬件约束通过临时 GPR 完成；不同类型的寄存器之间用 MOVD 保留 32 位原值。XMM 内部复制继续使用完整位复制，数据存储只读写该标量的 4 字节，不覆盖相邻字段。

整数加减乘与位运算、浮点加减乘除、min／max 直接取得右操作数，允许其为寄存器或溢出槽。两地址指令的结果若与尚未读取的右值共址，先在保留的临时寄存器完成计算，再交付结果；不通过交换浮点操作数消除复制。减法、除法、NaN 与正负零相关的 min／max 次序保持，CPU 的既有 SSE 行为遵循 [Intel 指令手册 MINSS／MAXSS](https://cdrdv2-public.intel.com/782151/253667-sdm-vol-2b.pdf)。这不扩大为所有后端的 NaN 载荷保证。

数值转换直接消费相应的分配位置。标量 CVTTSS2SI 可从 XMM／四字节源转换到实际 GPR；CVTSI2SS 写入实际 XMM 前清零其高位，避免旧目的值的依赖。u32 源按零扩展的完整32位正数转换，栈源只读取其四字节；bool 已规范为0／1，可与 i32 一样使用有符号整数到浮点转换。四元素 i32／bool 到 f32、f32 到 i32 使用对应 packed 指令直接读取来源并交付结果；u32 的高低16位与 2^31 拆分仍按原先的精确顺序计算。数值到 bool 仍以非零为真，f32 正负零为假、NaN 为真；位复制和数值转换继续分开，动态浮点到整数仍须满足原有效数值域。本项不改变算术顺序、ABI、公共 IR 或 GPU 要求，实际代码与点测记录见问题 C-002／C-024.3。
标量取负／abs 在实际结果 XMM 上用 XORPS／ANDPS 修改低32位，符号掩码从同一 NativeCode 的只读字池通过 MOVSS 读取四字节；位操作本身使用寄存器来源。固定来源直接交付变换后的原始字，真实共址溢出槽用四字节整数位操作，其余溢出仍使用原字临时。该降低保持NaN载荷、正负零、次正规数和其余31位，不执行浮点算术或数值转换。XMM未使用高位不改变标量存储／ABI；原分配图、调用保存、四元素降低、GPU与资源生命周期保持，掩码随原代码字节共同持有和释放。具体字节与点测见 C-002／C-024.3，不外推为所有程序提速。


整数除余使用编译期常量倒数的精确乘法／移位序列，动态移位使用 ECX／CL，结果直接交付实际位置。比较直接读取来源后生成原有 bool 结果；IR 已准备好两个输入的 Select 使用 CMOV 做位选择，不改变前端的条件求值与 if／while 控制流。B 发射器直接消费已分配的寄存器、栈位置、立即数及内存地址，按实际宽度写出标量复制和内存读写。窄整数装入进行符号或零扩展，窄写只更新对应的 1／2 字节，浮点位复制保留原位模式。ModR/M、REX 和 SIB 继续使用同一编码；f32 字面量由原常量池持有并接收实际 RIP 相对引用。

上述指令选择描述标量执行，四元素降低见下节。进一步的寻址、寄存器合并、活跃区间拆分和复杂控制流向量化未完成；代码更短也不自动证明某个程序吞吐更高。

### 四元素 SSE2 并行降低

普通直线 map 及批次内条件一致的分支／循环阶段可以在一个 CPU 范围函数内一次处理四个独立元素，范围内剩余至多三个元素执行同一 IR 的标量指令。选择在编译期完成，不增加源语言语法、运行时重编译、系统任务或另一套数据格式。现有 `simulation.bodies::step` 已经使用该降低；4 是当前 CPU 机器粒度，不是源语言 tile 宽度、GPU subgroup 宽度或光照采样数。

where 的直线及统一控制流正文复用相同计划、寄存器分配、Frame 和标量尾部。每范围仅用栈内 16 字节保存当前四个选中原索引，普通 Index 值、当前元素加载与独占写回均消费它们；不复制完整记录或建立全域 CPU 索引数组，资源与执行范围仍由原所有者持有。

编译器先区分常量／参数／只读统一地址得到的统一值，与逐元素值。存储必须使用当前元素索引；读取可使用当前元素、未被本阶段写入的数据集任意合法索引，以及不可变 view。各元素的独占写入、普通索引范围与绑定所有权沿用前端和执行器的既有证明；unsafe_read 仍由调用方承担外部索引关系，后端不扩大其权限。向量寻址还须能以目标机器的位移表达，阶段融合与显式 phase 保持。逐元素分歧控制流、tile／subgroup、射线查询与外部数学／C ABI 调用目前不进入四元素降低，这些阶段继续使用已有的标量范围生成。

只读间接读取逐 lane 生成 64 位主机地址，按实际绑定步长／字段位移取得四个标量后合并到既有 SSE2 批次。它不是硬件向量 gather 指令；批次中的算术、转换、bool 规范化与写回继续使用原有实现。嵌套 view 的地址高低字和元素索引可以每 lane 不同，结果在所有输入 lane 读取完成后才写回分配位置，允许真实的寄存器及溢出槽复用。一个读取操作的 view 基址统一时只构造一次该基址；地址和索引都统一时只读一次并广播。数据始终保持原 AoS 布局，没有预先打包整张几何、复制资源图或增加运行期索引检查。

四元素计划递归消费已检查的 If／Loop／Scope；只有全部跳转条件在批次内一致时共享分支、循环、break／continue 和作用域退出，未选分支不会执行。统一性由原索引／读写来源在实际程序点传递：局部改写更新当前值，SSA 定义保留其读取点的事实，不因该局部后续复用而改变早先循环的判断。普通继续边与 continue 参加循环回边固定点，break 与作用域退出分别在实际接收处合流；循环首轮一致而后续取决于各元素，或退出后用逐元素值决定控制路径时，仍保持标量。固定点稳定后才交付定义事实，嵌套试探不生产运行时数据。该分析不引入运行时检测、分歧掩码、另一套 IR 或资源状态；它仍可能保守漏失合法统一程序，不声明一般分歧 SIMD 已完成。不把有条件的外部读取提前到未选分支之外。普通索引由来源域、数值界／形状或显式条件证明；例如任意规模工作域的 index()%17 可以读取固定 17 项表，原同形域的 index()/3 也保持支持。含视图的值函数可在可表达的直线及统一控制流调用中批量降低，逐元素分歧及协作 SIMD 仍需继续实现。

已支持 f32 加减乘除、sqrt／abs／min／max，u32／i32 模 32 位加减乘、编译期非零常量除余与已逐分量证明非零的动态 u32／i32 除余、位运算、统一移位量，标量／bool 转换、六类比较和位选择。每个 packed lane 保留一个逻辑元素的值，浮点运算不重排、不横向归约、不融合成 FMA。u32 乘法用 SSE2 的偶／奇位乘积重组低 32 位；u32→f32 分别精确转换高低 16 位后只在最终加法舍入一次，覆盖大于 i32 最大值的输入。f32→u32 按 2^31 拆分再转换；动态转换仍需满足既有数值定义域，不承诺非法数值没有问题。

每个 XMM4–15 保存四个元素的同一标量分量，XMM0–3 为机器临时值；溢出槽为 16 字节。一个逻辑 float3 可以对应三个这样的向量，外部记录的 AoS 步长、字段对齐、填充与传输 ABI 不变。后端按字段装载四条记录，再按字段写回各自记录；只读统一数据只装载一份再广播。两地址运算保留未读右值和 min／max 的源顺序。

只服务寻址、没有普通值消费者的 Index 不生产索引向量，直接使用范围游标；实际使用 `index()` 时产生四个真实全局索引。整组入口以 64 位计算 index＋4，避免 u32 末端的加法回绕；只有整组落在范围内才装载四条记录。起点无需按四对齐，尾部不读取范围之外的行或广播记录。

同一干涉图先供标量着色，再供 SIMD 机器位置分析，分析消费者结束后释放。整组与尾部的临时数据区域可以复用，唯一 Frame 在最大区域之后保存两者实际使用的非易失寄存器，并在范围入口／退出各处理一次；不复制两套栈帧或改变资源完成点。该实现仅使用既有 x64 基线中的 SSE2；AVX2／AVX512、更宽批次、分歧掩码与协作 SIMD 仍未完成。

### 编译期常量整数除余

常量除数以 `IntegerDivisor::Constant(NonZeroU64)` 位模式进入同一 IntegerDivide，实际位宽和符号取自分子类型；常量不再作为运行期寄存器流转。动态除数则使用前文的 Value SSA 操作数。死值删除、表达式复用、局部值转发、活跃性与 tile 私有值保存共同消费这份 IR，不在两个后端重新建立常量事实表。

类型构造保留转换后各分量的常量信息，标量广播、嵌套向量和 swizzle 继续使用同一份信息。例如 `u32(7)`、`uint4(3, 7, 9, 65537)` 和 `int4(-1, 3, -7, -2147483648)` 可作为除数；包含任意零分量、转换后为零或没有当前非零证明的动态除数仍报 B152。整数常量表达式及不可变绑定保留结果，`3 + 4` 可作为除数；函数接口显式非零类型仍未实现，动态除数规则见前文。

CPU 标量与四元素 SSE2 共用自研整数倒数参数。二次幂以右移取得商；其他除数使用整数乘积高 32 位、必要的修正及右移，余数由 `n - q*d` 的低 32 位取得。不使用浮点倒数或近似除法；编译器的参数推导使用 u128，不代表源语言增加了 u64／u128 类型。SSE2 分别计算偶／奇 lane 的无符号乘积并重组高位，四个 lane 始终是四个独立元素。

有符号整数先以模 32 位运算取得无符号幅值，再恢复商的符号，保留向零截断及余数跟随被除数符号的语义。`i32::MIN / -1` 沿用取负的模 32 位结果，余数为零，前端仍直接生成该语义；不让 GPU 执行未定义的溢出除法。SPIR-V 从同一 IR 生成类型化常量与整数除余指令，驱动负责硬件指令选择。

逐元素不同移位量按原生标量范围生成；统一计数可使用已有四元素计划。更宽硬件指令和编译期成本选择仍属后端能力缺口，不增加运行时模拟切换。

### GPU f32 运算的保留模式

编译器从优化后的实际 IR 派生 f32 保持要求。二元运算在原语义生成处消费两个输入及结果的实际数值事实，包括原范围和无符号整数到f32转换所得正零或有限正规数；只有这些事实排除次正规、负零、NaN及无穷时，该次运算才不引入保持要求。比较只核对其浮点输入。未知输入或仍可能产生这些值时继续要求保持，不能只凭最终输出范围、样本或类型名称取消要求。数学函数、数值转换、块内集合、subgroup 数值协作和全域f32归约／扫描继续沿各自运算规则确定要求。

存在保持要求时，主入口、f32归约尾入口及扫描的递归／传播入口声明 `DenormPreserve 32` 和 `SignedZeroInfNanPreserve 32`；已证无需保持的二元运算、纯位复制、整数运算及已删除的浮点计算不引入该要求。程序设备合同与逐入口SPIR-V模式消费同一运算要求。模式不改变原运算顺序，不引入软件浮点、载荷扫描或用户开关。

同一 Vulkan 设备在既有属性查询链读取 `VkPhysicalDeviceFloatControlsProperties`，实际执行绑定检查所需的两个 32 位能力；不支持时明确报错。该能力来自 [Vulkan 1.2 浮点控制属性](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceFloatControlsProperties.html)，入口模式按 [SPIR-V float controls](https://github.khronos.org/SPIRV-Registry/extensions/KHR/SPV_KHR_float_controls.html) 编码。保留有符号零、次正规数、NaN／无穷语义不等于承诺所有 NaN 载荷、浮点结合顺序、近似数学结果和舍入误差一致；CPU 继续使用既有原生浮点指令。

保留模式的实际指令覆盖与除法精度分别解释。[Vulkan的SPIR-V环境](https://docs.vulkan.org/spec/latest/appendices/spirvenv.html)没有将OpFDiv列入必须遵守DenormPreserve的指令集合；其单精度2.5ULP条件只覆盖规范指定的除数范围。不能据开启上述模式宣称次正规除数的结果与CPU一致。具体设备的数值观测在台账记录；算法按真实数值尺度组织，不靠钳值或静默替换输出。

### GPU f32除法的当前范围
上述保留模式不将OpFDiv扩展成任意有限除数的全值域CPU精度。[Vulkan精度规定](https://docs.vulkan.org/spec/latest/appendices/spirvenv.html)对单精度除法的2.5ULP保证给出指定除数范围[2^-126,2^126]；不能只凭F32存储能表示某个数，就推出除法对该数有同样保证。编译器当前仍直接降低到对应原生指令，不声明统一正确舍入或全值域严格除法。

设备极端除数的实际行为、输入与失败记录见Issues的C-004；不可表示结果的溢出／下溢、其他数学函数和不同设备按实际后端范围处理，严格浮点策略与成本仍未完成。

## 通用原生执行入口

`brun <cpu|gpu> <module.b> <计算函数> <module-root> <output-directory> <binding.bldata|:count>... [texture.bltex...] [-- value...]` 直接编译并执行没有未满足前置条件的公开 计算函数。入口保持操作系统原生路径编码，源码、模块根、输出目录、数据和纹理文件不因Windows代码单元无法表示为Unicode字符串而拒绝或替换。程序名按语言文本解析，冒号前缀计数与接口标量按原数值语法解析；已写冒号的非法计数不会改走文件读取。入口先接收精确的`cpu`／`gpu`后端；其他值直接交回后端参数错误，不编译模块、读取数据／纹理或生成程序描述。先按接口 dataset 顺序绑定数据，再按纹理参数顺序提供与其 texture2d／texture3d 声明维度匹配的 `.bltex`，与参数在原接口中是否交错无关；`:count` 明确申请该长度的全零数据。每个值参数按接口类型解析，向量按分量顺序提供。只写与读写数据输出为 `<参数名>.bldata`，read 数据不重复保存。加速结构和 view 通过原生 Bindings API 绑定，不从数据文件伪造地址。

```powershell
..\BLEngine\target\release\blengine.exe simulate cpu 1048576 8 Saved\bodies.bldata
..\BLEngine\target\release\blengine.exe compute gpu ..\BLEngine\Programs\simulation\statistics.b step_and_total ..\BLEngine\Programs Saved\Compute Saved\bodies.bldata :1 -- 0.008333333 0.2
```

`BLDATA v1` 为小端：8 字节 `BLDATA\0\0`，随后 u32 version=1、记录数、记录步长、执行步数，再跟完整记录字节。它保存指定编译布局下的值，类型和字段含义来自 `.h`；文件本身不能证明相同步长的数据具有相同业务类型。读取先核对magic／version与实际绑定，再借用原绑定Ty的size／align核对记录步长；对磁盘文件，从同一已打开句柄取得的总字节数必须至少为24加记录数乘步长，否则在构造记录载荷前返回UnexpectedEof。Windows管道元数据的零长度不代表没有载荷，读取按实际句柄种类区分，其他可读来源仍沿同一read_exact消费。磁盘长度只是当次观察，不能替代实际读取：合法载荷沿同一read_exact直接填入最终已初始化字存储的逻辑字节区间，再原地按小端规范化；不建立独立载荷缓冲或逐字临时数组，末字未读填充保持零。完整读取和未声明尾部仍单独判断，读取期间变化产生的真实I/O错误照常返回。零记录和零步长允许零载荷，末字填充不成为记录；不增加记录数上限，执行绑定继续检查类型和形状。写出沿原字存储和BLDATA小端格式交接：小端目标直接借用逻辑载荷字节区间批量write_all，其他字节序保留必要的逐字小端编码；末字额外填充不进入文件。原写缓冲合并头部与小写入，实际write_all和flush错误继续返回，不另建整载荷副本。`simulate`与通用入口共用`Dataset::read_data/write_data`。

Blink文件运行入口为`brun [--gpu-timestamps] <cpu|gpu> <module.b> <program> <module-root> <output-directory> <binding.bldata|:count>... [texture.bltex...] [-- value...]`。默认GPU运行不创建／重置时间戳查询池，也不写入／读取时间戳；显式`brun --gpu-timestamps gpu ...`请求采集，此选项不用于CPU。成功执行但没有计时样本时输出`GPU_program_timestamps=not_collected`，不输出伪零；有样本时的`GPU_program_ms`只含设备程序和阶段依赖；CLI 的 `compute_elapsed_ms` 从程序编译结果与输入数据准备好之后开始，包含后端编译、执行和输出写盘，GPU 另含设备创建／上传／读回。两者不是相同口径，也不能与 CPU 原生运算时间直接相减计算加速比。

### CPU数据字存储的按需查询

Rust SDK的`Dataset::memory()`直接读取该对象当前的原始字存储，返回`DatasetMemory`。`ReadDataset`和`cpu::CpuDataset`通过原Deref使用同一查询，冻结／只读绑定不复制数据。

| 字段 | 字节口径 |
|---|---|
| `logical_bytes` | `count * stride`，包含类型布局与记录步长的填充，不是逐分量有效值之和 |
| `stored_bytes` | 原`Vec<u32>`当前长度乘四，包含最后一个u32中的尾填充 |
| `capacity_bytes` | 原`Vec<u32>`当前可用容量乘四，不含分配器管理开销；不表示OS承诺页、工作集或峰值 |

例如一百万零一条一字节记录的逻辑跨度为1,000,001字节，打包长度为1,000,004字节；生产者交入的Vec可以有更大容量。缩容只改变实际长度，当前Vec保留的容量仍如实报告。零记录也可能持有生产者预留的非零容量，不能据逻辑字节为零推断没有分配。

`application::Frame::cpu_data_memory()`返回当前Frame可见CPU数据字存储的`Vec<FrameCpuDataMemory>`，各项的`handles`为原Frame句柄，`memory`为同一`DatasetMemory`快照。共享槽按同一实际Dataset分组；分组只属于本次查询，不建立新的资源身份。不同的空Dataset仍分别保留原句柄，不能用空Vec的悬垂指针把它们合并。已释放槽不再出现在新结果中；GPU驻留槽不在这份CPU范围中，查询不会等待或隐式download。

```rust
let current = dataset.memory();
for storage in frame.cpu_data_memory() {
    println!("handles={:?} bytes={:?}", storage.handles, storage.memory);
}
```

结果仅保存本次同步读取的元数据，不持有Dataset、设备或完成对象；保留旧结果不延长资源寿命，也不保证下一次resize／驻留变化后的状态。该范围不遍历view来源图，不统计代码VM、类型／句柄容器、Vulkan映射内存或其他宿主分配。Rust SDK与B／C入口分别记录：本查询没有新增B语法、C ABI或运行库状态字段。更广观察范围继续见Issues C-021.7.9.23。

### CPU数据的布局与view来源借用

Rust SDK的`Dataset::layout() -> &Ty`借用同一存储的原类型；原`count()`、`stride()`、`words()`和`memory()`分别交付记录数、物理记录步幅、原字存储与直接字节口径。`ReadDataset`／`cpu::CpuDataset`沿原Deref使用同一接口，不复制布局或载荷。

`view_bindings()`返回精确长度迭代器，每项`DatasetView { record: u32, offset: u32, source: &Dataset }`直接借用原Views的一条绑定及其真实来源。`offset`以字节计，相对本条记录起点；不是整个数组偏移，也不是来源内偏移。按原绑定顺序枚举，来源可继续调用这些接口，沿实际闭合图观察。多个字段／记录引用同源时，`source`是同一个Dataset；不同空Dataset不能按其空Vec指针合并。对象引用只在本次原借用有效期内比较，不作为持久资源ID或跨驻留身份。

`view_source(record, field) -> Option<&Dataset>`使用原Ty活动字段路径和真实bindings二分查源，例如`pair.left`、`list[1]`、`Some.left`、`Ok.right`；字段名保留大小写。它只返回实际绑定的view来源，记录越界、未知／非view字段、非活动载荷及已撤回绑定均为None。原始header即使保存非零地址，也不被当作来源；原`words_mut`撤回绑定后，查询如实无来源；`set_scalar`只更新原Ty选中的单标量字段，未改变的view绑定与来源继续可查。空view来源仍为Some，其count可以为零。

```rust
let ty = data.layout();
for view in data.view_bindings() {
    let byte = u64::from(view.record) * u64::from(data.stride()) + u64::from(view.offset);
    let source_memory = view.source.memory();
    // byte对应本数据集的实际引用字段；source_memory只描述来源自己的字存储。
}
let source = data.view_source(0, "pair.left");
```

以上接口不分配观察图、不克隆Arc、不读回／等待GPU、不修改记录或保活另一份所有权。返回借用由Rust约束原父存储与来源寿命；不能据查询取得可写来源、跨越父存储释放或在活动观察时改写父存储。CPU执行本身沿原同步入口完成，查询不伪造GPU最后使用状态、初始化许可或静态别名事实。GPU对应的来源与绑定完成点、Frame数据根见后文。B／C对应的来源查询入口仍待实现。

### GPU布局、view来源与绑定保留点

Rust SDK的`vulkan::GpuDataset::layout() -> &Ty`和`stride() -> u32`交付原物理GpuStorage的类型布局与记录步幅。`view_bindings()`返回精确长度迭代器，元素为`vulkan::GpuDatasetView { record: u32, offset: u32, source: &GpuDataset }`，直接借用原Views的实际边和原Rc中的来源；offset是相对本条记录起点的字节偏移。嵌套来源可继续查询，不解析设备地址、不保留CPU记录镜像或克隆Rc。GPU不提供CPU式的活动字段路径查源：静态Ty和实际绑定偏移分别交付，不读回活动变体tag，也不把两个共用偏移的候选字段同时当成活动字段。

`same_storage(&other)`只比较当前有效借用中的同一原GpuStorage；它不比较外层GpuDataset句柄、类型、页或共享VkDeviceMemory块。冻结别名为true，不同空数据仍为false。存储移动或原所有者结束后不能把旧对象地址作为永久身份。`vulkan::ReadDataset`通过原对象Deref使用这些查询，现有memory／count／bytes／allocated_bytes／page_count和binding入口保持。

`last_use() -> Result<Option<vulkan::GpuDatasetUse>>`查询本绑定的原dependency：先取本次已记录last_use，无本次点时沿同一冻结来源。Some包含device、sequence和complete；device是原VkDevice的当时作用域，sequence是该原队列从0开始的提交序号，complete由同一原Execution::is_complete观察。不同逻辑设备序号各自从0开始，不跨设备排序；值也不作为持久执行ID。完成状态是查询当时的事实，GPU仍可继续执行，标准状态语义见[Vulkan vkGetFenceStatus](https://docs.vulkan.org/refpages/latest/refpages/source/vkGetFenceStatus.html)。None仅表示没有保留点，可能是尚未提交或显式wait已清除本次点，不证明数据初始化、设备空闲或别名全部完成。

同源只读绑定可以记录不同的真实执行点：源保留生产点，后续每个绑定保留自己的读取点。`same_storage`成立不合并这些点；源上的旧点完成不代表另一个绑定的新工作完成。成功Submission::wait清除本次绑定点，Owned可变为None，Read仍可能沿原冻结来源观察较早的点；它不是已被清除工作的第二记录。

查询不提交、等待、下载或改变producer／绑定点，也不增加完成表／状态镜像。原Execution可以按其已有职责清理实际已经完成的命令资源；原生错误直接返回Err，不写成None或complete。GpuDatasetUse只复制本次元数据，不持有Execution／数据／设备；保留旧快照不会保活资源、阻止后续提交或改变释放时机。

```rust
for edge in gpu_data.view_bindings() {
    let position = (edge.record, edge.offset);
    let original_source = edge.source;
}
if let Some(point) = gpu_data.last_use()? {
    // point只描述这个绑定实际保留的device／sequence及当时完成状态。
}
```

本节是原生Rust SDK观察，不新增B语法或C ABI。Frame当前数据根见下一节。B／C对应的来源查询入口仍待实现。

### 应用Frame数据根与有效域关系的借用

Rust SDK的`application::Frame::data()`返回`&DomainBindings<Option<application::ResidentData>>`，直接借用同一个Frame的原data。`datasets()`返回原`&Vec<Option<ResidentData>>`，保持原槽索引和已释放None空槽，并提供原容器的len／capacity；不复制、过滤或生成句柄目录；ResidentData::Cpu和Gpu分别持有原CpuDataset／GpuDataset，借用可继续调用原布局、来源和存储查询。观察不load程序、创建设备、执行、等待或传输，也不改Frame状态。

活动共享借用期间不能run／allocate／resize／upload／download／release或先行释放Frame。每次变更结束后重新借用当前根，实际CPU／GPU驻留由原枚举交付；不得持有旧地址跨越可能移动的操作。句柄整数仅索引这个Frame当前槽，release后为None，后续allocate可复用该整数而持有另一数据／类型，不据旧句柄推断身份或存活。当前Frame公开生产路径没有接入外部with_views／冻结数据的入口，查询没有增加插入器或伪造该集成；已有后端来源能力分别保持。

数据槽容器的len包含None，不是当前活动数据集数量。对非零尺寸D，len／capacity乘size_of::<D>()分别给出已初始化槽跨度／原Vec可用容量字节；这些字节不包含D内部拥有的数据字、代码、类型或设备资源，也不表示分配器额外开销、OS页或全Frame占用。capacity尾部不能读取为已初始化D。零尺寸D的槽字节为零，不能把Rust Vec的usize::MAX容量当实际字节分配；空容器指针也不是分配证明。release可保留原槽容量，实际CPU／GPU载荷及完成仍按原所有者处理。共享借用不允许修改原容器，精确Rust返回签名为&Vec<D>；as_slice或原强制转换可交接已有切片消费者，不增加旧签名入口或跨模块Rust容器ABI。

`DomainBindings<D>::domain_relations()`返回同一原domains的`&Vec<DomainRelation>`，Frame的data同样使用该入口。len为当前有效关系数，capacity为原容器可容纳的关系槽数；分别乘`size_of::<DomainRelation>()`表示有效槽跨度和容量字节，不包括变体内部区间载荷、数据载荷、分配器或OS开销。Count写入、字段覆盖或release可撤回对应关系，同时保留容器容量；预留槽不是有效关系，也不发布许可。共享借用可通过as_slice或原强制转换交接切片消费者。DomainRelation的Prefix与Element变体共用同一原容器：

| 变体／字段 | 来源和单位 |
|---|---|
| Prefix.domain | 当前原datasets中的实际槽索引；Frame中即原句柄 |
| Prefix.offset／bytes | 完整记录内选中字段的字节偏移／声明类型字节数，不是元素范围或容量 |
| Prefix.count.binding | 同一原datasets中实际计数存储的槽索引 |
| Prefix.count.word | 计数字段在记录内的字节偏移；虽然名为word，也不是u32下标或计数值 |
| Element(ElementRange)中的binding／offset | 同一原datasets槽与记录内实际字段字节偏移 |
| Element(ElementRange)中的scalar／intervals | 原Scalar与完整字段已生产的有序区间集合；编码见持久字段索引前提一节 |

例如带八字节普通前置字段的记录，其计数字段字节偏移为八，Prefix.count.word就为8；不能将其当第八个u32。Element的intervals是原Vec，其len／capacity分别乘`size_of::<(Bits,Bits)>()`给出有效端点槽跨度与容量字节，不包含分配器开销。Frame把本次逻辑参数映射到原槽后交接同一关系，字段投影保持实际字节偏移。CPU／GPU prefix生产、CPU完整字段区间生产、对应字段真实写入、resize／release及DomainBindings的重排／分区沿原关系生产／撤回规则执行；查询不复制或重建另一份权威，GPU字段完成与传输已由原统一链交接，核对记录见FixedIssues C-021.1.4.5.2.3；本查询只借用原关系与槽，不签发事实或完成。

```rust
use blcompute::application::ResidentData;
let root = frame.data();
for (handle, slot) in root.datasets().iter().enumerate() {
    match slot {
        Some(ResidentData::Cpu(data)) => { let layout = data.layout(); }
        Some(ResidentData::Gpu(data)) => { let source_edges = data.view_bindings(); }
        None => { /* 当前已释放的原空槽 */ }
    }
}
let actual_relations = root.domain_relations();
```

关系和数据容器仍由原拥有者私有持有；公开DomainRelation的读取／普通值构造不授予向原domains发布关系的权限。旧元数据副本不替代当前require，也不证明GPU实际完成、B初始化或容量尾部可读。GPU异步绑定的完成关系继续由原last_use解释，Frame::run保持原同步调用；这些查询是Rust SDK接口，不新增B语法或跨单元C ABI容器。Frame程序根见下一节；B／C数据与代码观察仍沿各自原未完成范围。

### 应用Frame类型声明容器的借用

Rust SDK的`Frame::types()`返回构造时移入的原`&Vec<Ty>`，索引对应本Frame的类型号；len为有效类型数，capacity是原容器可容纳的Ty槽数。原Ty的size／align／components继续描述B数据布局，不是Rust Ty对象的槽大小。len与capacity分别乘实际`size_of::<Ty>()`得到已初始化槽跨度与容量字节，只计外层Vec，不含Ty内部String／Vec／Box、数据字、代码、设备、分配器或OS开销。capacity尾部不能作为有效Ty读取，空Vec指针不证明已分配，空但预留容量仍可持有槽存储。

查询共享借用原表，不复制、过滤、修改类型或创建另一目录；原表保持到Frame结束。活动借用不能跨Frame修改或销毁，数据分配／resize／驻留替换／release结束后再借用同一表；这些数据操作本身不重建类型表。查询不装入、执行、等待或传输，也不赋予写权限。返回值可按原Rust规则借用为切片，不跨单元传递Rust容器，不增加B／C观察ABI。其余宿主分配与跨语言观察仍待实现。

### 应用Frame程序槽与代码根的借用

Rust SDK的Frame::programs()返回原模块目录的&Vec<ProgramSlot>，索引保持真实程序号，len／capacity乘size_of::<ProgramSlot>()只计这个原容器。ProgramSlot::Cpu持已经绑定的CpuTemplate，Gpu持原只读映像；类型和代码目录由原模块持有，Frame不在run中取出／归还CPU程序。Frame::new(types,images)消费这些定义并返回Result<Frame>，CPU装入错误在构造阶段交回，未形成Frame的局部资源正常释放。

Frame::gpu_programs()返回本Frame实际&Vec<Option<GpuTemplate>>，同一程序号对应本设备实例，CPU位置没有GPU实例；len／capacity乘size_of::<Option<GpuTemplate>>()只计该容器，不能把None计作已创建的GPU程序。GPU在真实需要时从共享定义装入pipeline，原SPIR-V和同一ExecutionPlan不复制成另一权威；设备、pipeline及完成仍归实际GPU消费者。

ProgramSlot::layout()共享借用原ExecutionPlan。查询原定义、GPU槽和类型不分配、装入、创建设备、执行、等待或传输，也不授予改写私有容器的权限。活动借用不跨Frame修改或销毁；CPU已装入和GPU槽Some均不表示工作接收、执行或完成。

```rust
use blcompute::application::ProgramSlot;
for (program_index, slot) in frame.programs().iter().enumerate() {
    let actual_plan = slot.layout();
    if let ProgramSlot::Cpu(code) = slot {
        let current_vm = code.code_memory()?;
    }
    let current_gpu = &frame.gpu_programs()[program_index];
}
```

CPU code_memory仍是原按需Result<Vec<CpuCodeMemory>>，返回Vec会分配并调用VirtualQuery，真实成本和原OS错误保持；报告不保活代码或授予写权限，多个入口共用区段不能重复计成独占内存。数据release不释放仍有真实消费者的定义／代码，GPU最后使用点仍保持相应设备资源。此节为Rust SDK接口，原资源字节编码和共同C ABI没有因内部Arc或容器移动而变化；原CPU实例输入和执行失败返回的有限范围已经交付，证据见FixedIssues C-027.35.4.2.2.3.2.2及C-020.5.2.4.2；普通拥有返回及内部CPU子域沿同一私有Frame、已绑定程序和原完成出口执行，不另建上下文或完成链。

### 原生物理 GPU 枚举与选择

Rust SDK 的 `vulkan::Instance::new()` 创建计算实例；`Instance::with_presentation()` 明确启用 Windows surface。两者的 `physical_devices()` 都返回实际 `Vec<PhysicalDevice>`，包含集成 GPU 等驱动可访问类型。`PhysicalDevice::name()` 供显示、`is_discrete()` 供调用者选择；名称不是资源身份。保留选中的句柄后可释放列表和公开实例，真实实例由该句柄及其派生设备共同持有。

`physical.device()` 创建基础计算设备；`physical.for_program(计算函数)`／`for_programs(programs)` 合并实际程序所需能力并在该物理设备上创建独立逻辑 `Device`。呈现能力沿原实例的显式创建方式继承；其余设备／程序／格式边界仍由原入口负责，缺少能力直接报错。原 `Device::new`／`for_program(s)` 及 `with_presentation(_programs)` 将全部物理候选交给同一创建入口，保留独显优先次序，同类沿驱动枚举顺序，选择首个满足本次已声明特性、计算队列及必要设备扩展的设备；设备类别不排除候选。显式PhysicalDevice入口只传入该选定项；多程序需求在候选查询之前合并，浮点保留模式和全部subgroup能力均由原DeviceRequirements交接。每个候选只在创建之前查询和匹配，逻辑Device及真实资源保持确定的物理身份；原生查询、创建、装入和执行错误如实返回。原VK_EXT_memory_budget支持事实来自同一次通用扩展枚举，预算入口仅持有原只读查询。推送常量容量在同一程序遍历中消费实际uniform_bytes的最大值，在创建前匹配候选已查询的硬件属性；装入时继续以已创建的实际设备核对。纹理描述符容量在该遍历中按每个原布局的数组元素总数取最大值，匹配候选原stage／set的sampler及sampled image四项属性。实际装入按该设备核对本次程序，局部纹理总数供同次纹理和组合资源检查共用，不保存为另一份计数权威。GPU设备创建借用实际将装入映像的ExecutionPlan，默认、显式物理设备与呈现入口共用该交接。阶段workgroup_width及发射后的shared_bytes分别按各阶段、各独立程序取最大值，直接匹配候选已有maxComputeWorkGroupInvocations／maxComputeWorkGroupSize[0]及maxComputeSharedMemorySize；不相加独立程序的宽度或共享存储。后续装入继续核对已确定设备的实际容量。同一创建遍历复用每个程序的texture_count，将实际普通及辅助storage binding数量与纹理数组元素总数求和，再按独立程序取最大组合数，直接匹配候选已有maxPerStageResources。combined image sampler每个数组元素计一项，AS不进入该项；不能将不同程序各自最大的storage与texture数量拼成不存在的布局。同一程序遍历只求一次普通buffers＋auxiliary_buffers的storage描述符数量，供该程序组合总数及独立storage最大值共用；多程序取最大storage数，在创建前同时匹配候选已有maxPerStageDescriptorStorageBuffers与maxDescriptorSetStorageBuffers。装入已有设备时在原入口一次求storage数，核对同样两项容量，并供组合资源总数复用。AS绑定数直接来自各程序accelerations，多程序取最大值；原AS物理属性并入创建前已有subgroup／float／sorting查询链，同次匹配maxPerStageDescriptorAccelerationStructures和maxDescriptorSetAccelerationStructures，再把同一属性移交原RayDevice。装入沿该设备已有属性核对相同两项容量，不再另查一次AS属性；没有改变原AS构建、完成和资源释放。

```rust
let instance = blcompute::vulkan::Instance::new()?;
let physical = instance.physical_devices()?;
for selected in &physical {
    let device = selected.for_programs([&step, &shade])?;
    // 编译、上传与提交仍使用原 device 接口，资源只属于该逻辑设备。
}
```

这项 API 是原生执行框架的物理设备入口，没有新增 B 语法或设备编号字段。不同物理 GPU 可以沿 `source.download`／`target.upload` 或纹理对应入口显式通过 CPU 交接；返回 CPU 数据与目标 GPU 存储分别所有。`same_device` 比较真实逻辑设备及队列所有者，同一物理 GPU 上独立创建的设备也不相同。直接设备间传输、多队列、自动分片／迁移及完整多设备语言调度仍未完成。

## 语言优先的开发顺序

Blink 是引擎主要计算算法及其生产消费关系的表达语言，覆盖适用的 CPU／GPU 计算。材质与光照、图像计算、几何与空间算法、动画与模拟、场景批量数值更新、有效工作组织和阶段组合主要使用 B；Rust 继续负责自研编译器与优化后端、原生执行框架、平台接口和物理资源生命周期。系统边界与核心设计以 引擎内部架构资料（不公开） 的对应章节为事实源。当前渲染求交、GGX、阴影、resolve 与模拟程序已经走这条链路；完整分工包含尚待实现的能力。

B 表达数据域、访问与协作语义以及算法依赖，Rust 编译器与执行框架据此落实原生执行。几何页需求与优先级等计算归 B，实际 I/O、分配、传输、Vulkan 命令、硬件加速结构构建和实际完成后的释放仍由 Rust 的既有所有者负责；BLEditor 编辑与排版继续属于原核心。两者不维护第二套场景状态或宿主计数。当前语言和后端能力缺口由真实消费者推动补齐，不能当作主要算法永久留在 Rust 的边界；沿既有 计算函数／IR／执行链替换重复实现，不新增独立调度服务、宿主逐元素回调或额外校验流程。

语言发展的运行效率目标是跨阶段消除无必要中间存储、按实际有效域推进工作、改善数据局部性，并让执行框架减少无必要的交接与等待。CPU／GPU 后端持续提高机器执行质量；不以 B 源码比例、内核数量减少或当前单项实现代替同画质、同采样与同几何精度下的产品性能结果。

有效前缀、扫描／压缩、数值键排序、分段集合、块内集合及同队列资源汇合／分叉按本文相应章节提供。动态工作量、容量、唯一写入与资源所有权进入真实语言／IR语义，不能只用函数名包装任意散写。当前提交顺序由宿主确定，通用设备自主任务与跨设备调度仍未实现。

CPU 后端已有物理寄存器分配、溢出槽复用和直线逐元素四元素 SIMD，继续活跃区间拆分、寻址／指令选择、复杂控制流与更宽向量；GPU 已在 subgroup 值协作和类型化 tile 输入之上实现中间值发布及块内集合原语，继续补可变协作、同步作用域与调度优化；数据与依赖证明成立后再融合。静态 cooperative 函数及宽度继承现已实现；模块增量编译、语言资源泛型、调试信息和大于 u32 的记录域仍需完成。先把语言与执行基础用于真实消费者，再继续资产导入和上层引擎功能；FBX 与 UE 转换需求保留。

## 开放 u32 const、静态形状推导与具体入口
函数、cooperative、计算函数 和记录可以声明 `const N: u32`，无需枚举长度。N 在每次具体使用时确定，覆盖 u32 值域；0 是否合法由实际类型、操作或已声明约束决定。普通数组允许0，纹理数组和 tile 宽度仍按其实际规则要求正长度；普通数组与记录布局继续检查原 u32 字节范围；静态参数不意味着设备能承载所有长度。类型参数仍使用原有限类型集合，开放 u32 常量参数不等于开放类型接口。

开放与有限const可以组合。显式类型／值实参使用同一顺序，const位置可写能完全编译期求值的u32表达式；具体类型／const代入后按原常量语义取得值，身份仍规范化为实际类型名与十进制值。形状表达式不引入运行期形状、宿主镜像或一般符号方程求解，也不从派生形状反推const参数；需要时显式传入实际const实参。没有默认值、范围表达式或一般符号谓词。

不显式指定 const 时，编译器可从对应的固定 array<T,N>、名义泛型记录、固定 dataset<T,N> 及 read array<texture2d,N>／read array<texture3d,N> 的静态形状推导。多个实际参数对同一 N 必须一致，冲突报 B235。动态 dataset 的运行期容量、GPU 的有效 Count、tile 的有效尾长度和纹理像素不会被读回或用于猜测静态参数；无法推导时必须显式给出。函数、协作调用与 计算函数 阶段调用共用原实例化和实际资源身份。

例如 `u32 divided<const N:u32>(u32 value) on compute requires nonzero(N);` 的 N 在实例化时直接检查；N=0 报 B226，非零时该已满足的 const 条件不进入执行接口。一般值参数的 nonzero、nonempty、index 和 prefix 仍按原规则证明，不能从静态参数伪造实际有效域。

完整 `.h`：

```c
module open_blocks;
struct Cell<T:numeric> { T value; u32 audit; };
cooperative T plus<T:numeric,const Bias:u32>(tile<T> input) on compute;
void run<T:numeric,const Width:u32,const Bias:u32>(inout dataset<Cell<T>> rows) on compute;
```

对应 `.b`：

```c
module open_blocks;
cooperative T plus<T:numeric,const Bias:u32>(tile<T> input) on compute {
    T value = tile_reduce_sum(input);
    return value + T(Bias);
}
void run<T:numeric,const Width:u32,const Bias:u32>(inout dataset<Cell<T>> rows) on compute {
    parallel (row in rows) {
        let block = tile_input(rows.value, Width);
        T value = cooperate plus<T,Bias>(block);
        row.value = value;
    }
}
```

bc 可选择公开 计算函数 的具体入口：

```powershell
.\target\release\bc.exe check <module.b> <module-root> 'run<uint3,37,11>'
.\target\release\bc.exe build <module.b> Saved\Compiled <module-root> 'run<uint3,37,11>'
```

Rust API 为 `blanguage::compile_entry(path,module_root,"run<uint3,37,11>")`；入口必须公开且属于入口模块，可用限定名和模块 const。返回的 programs／templates 只包含所选具体入口，名字使用规范化类型与值；原类型、访问、索引、初始化、阶段和 prefix 检查完成后才产生 ExecutableProgram 或 TemplateProgram。条件模板仍需真实 DomainBindings，普通 compute 文件入口不能绕过。BLEngine compute 命令使用相同入口选择，无需先在源码中写一个具体包装 计算函数。

不带具体入口的 check／build 保持有限实例检查；开放声明不枚举 u32 全域，也不隐式选择一个值。其通用声明结构与固有资源权限先检查，依赖具体值的布局和函数体在实际使用时完整编译检查。Compilation.open_generics 与 bc 输出列出开放声明，并明确 body checked on instantiation；只检查未使用开放库不证明所有可能实例的正文都合法。只有开放 计算函数 且没有可导出具体入口时，build 报 B240，要求选择入口。完整符号泛型体分析仍未实现。

开放参数使用原 AST 替换、名义布局与 IR 常量，使用后即为具体算法；不增加泛型运行时、宿主逐元素回调、动态编译服务、额外 uniform、运行期长度表或另一份资源状态。material.textured::indexed_ggx 已删除原固定长度清单，从实际纹理数组推导 N，索引仍必须满足 requires index；实际算法、资源完成和像素品质目标保持。

## 共享编译会话与明确本机资源（C-021.5.2b.1，已实现并完成当前范围原生验证）

Compiler由SDK调用方持有，其clone共享同一CPU工作许可。创建普通会话用Compiler::available()，或以NonZeroUsize明确Compiler::with_workers的并行预算；Windows按Processor { group, number }列表调用Compiler::on_processors选择实际逻辑CPU。预算不是固定32，组内编号不是全机编号。OS绑定错误直接报告，不改用其他CPU。其他平台的显式CPU绑定仍未实现；具体机器验证范围见双台账。

现有源码函数对应同会话方法compile、check、compile_entry、compile_application、compile_application_with_modules及compile_native_module。CPU与SPIR-V映像使用compiler.native(program)／compiler.spirv(program)，正式入口复用使用compiler.build_entry(path,root,entry,target,output)，消费编译后obj／lib时使用同一会话的build_entry_with_inputs(path,root,entry,target,output,inputs)，算法库对象使用compiler.compile_dll(program,prefix)。从静态库查询公开接口时借用成员原字节，按对象元数据与实际导出取得接口和合同。链接及栈诊断由各自所需的完整对象材料交接。返回的App／Module的compile继续消费其创建时的会话；待发射物不持有活动执行许可，最终机器产物不带编译会话。需要跨调用共享预算时复用同一Compiler，多个自由函数调用不会偷偷成为全局预算或常驻服务。

每个活动编译及内部独立阶段只使用已分配的真实许可；没有空闲许可时在当前工作完成，内部不等待父工作已经持有的许可。普通调用在原调用线程执行，显式CPU绑定使用本次临时工作线程，所有派生消费者汇合后才归还资源。运行库静态目标代码仍在编译阶段生成，其Cargo子工作使用当前这一份CPU许可，不另行扩大整机并发。已存在的完整模块推导、资源和Frame／Device职责保持。

正式CLI在命令前声明资源，例如：

```text
bc --jobs 32 -c app.b -o app.obj -I Programs --entry main
bc --cpus 0:2,0:4 check app.b Programs
```

--jobs与--cpus表达两种互斥的资源声明；它们经同一Compiler进入语义检查、代码生成和现有导出。blink继续独立消费完成对象，不以此重读B源码或编译代码。单次工作并行不等于完整多模块图、精确增量或域编译，剩余能力继续按Issues记录。

## 单次本机编译的独立工作并行
现有compile、check、compile_entry及应用入口使用本机独立工作并行，不需要新源语法。原装入器完成源码、接口和调用参与推导后，Declarations只读共享；每个worker的World只持有同一份定义与自己的const递归求值状态。有限声明按原顺序展开为闭合实例，各实例可独立检查与优化，同一泛型声明不再把全部实例绑在一个worker上。具体入口仍完整检查；不以并行省掉非法声明或访问／阶段边界。声明展开错误进入同一有序结果，早先函数体的真实错误仍优先报告。

没有输出消费者的有限整域声明仍完成相同语义与单映像B270检查，随后不再优化要丢弃的IR。被正式保留的入口继续使用原优化器。普通模块先完成原共享函数／资源合同收敛，再并行独立函数的机器码发射；CPU／SPIR-V的程序阶段并行生成。模块资源映像按实际目标／阶段批量发射，全部完成才按原次序组装，不在每个worker里再创建整机规模的线程组。

本次阶段只拥有真实工作队列和临时上下文；工作／产物按原顺序汇合。错误保留原源顺序的最早诊断，已启动消费者结束后释放共享输入；线程资源错误直接报告，不重试或转为另一路编译。worker上限来自同一Compiler的可用执行许可与工作数量，普通会话默认预算由Rust可用并行度估计，并行度来自实际会话资源，不以某台机器的处理器数规定语言上限。

native::compile_program与spirv::compile_program的Rust错误结果现为String，可完整表达本机线程资源错误；原静态发射错误文字保持。Frame、Device与C绑定职责保持，blink仍只链接已编译对象，运行不增加编译器或调度进程。

跨SDK调用共享预算、明确CPU选择及显式多单元构建图按相应章节提供，BLEngine沿真实准备依赖采用同一Compiler。完整精确增量、域编译和跨机运行仍未实现。单个强依赖入口的语义／优化成本不因本项自动消失，不把小入口的并行结果称为完整4K冷启动达标。

## 本次多单元构建图（C-021.5.2b.2）

调用者用build::Unit声明本次编译单元、路径、模块根、依赖和输出，在同一Compiler上调用build_graph(&units)，或使用build::compile_graph。UnitKind::Object(ObjectKind)沿原对象构建；UnitKind::Entry { entry,target }沿原CPU／GPU入口映像构建。普通模块、应用、算法库对象及Entry映像以Dependency::Unit(index)引用本次提供者，或Dependency::Object(&object)借用已有模块，传递模块依赖自动沿这些绑定交接；图输出保持原所有权，入口在原选择完成后取得最终存活代码，不深复制整对象或运行库归档。实际源码按路径在本次图中捕获一次并共享；接口绑定按每个编译单元所选提供者交接。同名同文接口来自不同文件或来自Unit／Object时，不能互借来源。Dependency::Unit共享自己已解析的接口快照，后续声明／类型诊断保留该提供者真实路径、字节和范围；Dependency::Object只有接口文本时继续使用<模块名>.h，不推测或读取提供者源码。同一虚拟Object接口可共享解析结果，各单元的绑定、实际代码提供者及原依赖关系仍分别保持。源码和绑定保持到图内最后真实消费者结束，路径描述这次捕获，不表示重读当前磁盘。单元引用可以向前，真实循环及越界直接报告；入口映像不能当作COFF模块提供者。

例如已有math与app两份模块，可由实际构建调用组织：

```rust
use blanguage::{Compiler, build::{Dependency, ObjectKind, Unit, UnitKind}};
use std::{num::NonZeroUsize, path::Path};

let root = Path::new("src-b");
let math = root.join("math.b");
let app = root.join("app.b");
let math_object = Path::new("target/graph/math.obj");
let app_object = Path::new("target/graph/app.obj");
let app_imports = [Dependency::Unit(0)];
let units = [
    Unit { path: &math, root, kind: UnitKind::Object(ObjectKind::Module(&[])),
        dependencies: &[], output: math_object },
    Unit { path: &app, root, kind: UnitKind::Object(ObjectKind::Application("main")),
        dependencies: &app_imports, output: app_object },
];
let compiler = Compiler::with_workers(NonZeroUsize::new(32).unwrap());
let outputs = compiler.build_graph(&units)?;
```

产物之后由独立blink消费：

```text
blink target/graph/app.obj target/graph/math.obj -o target/graph/app.exe
```

Entry单元可与这些对象放在同一图中，并以同一个Dependency::Unit索引引用实际模块；其output是BLBI目录。外部模块用Dependency::Object借用，提供者h／b无需存在。多个入口只共享原ObjectCompilation的Arc，不改写原节、重定位或归档；每个完成映像拥有自己的实际链接材料，图借用结束后可独立装入。公开合同改变重编根，私有代码改变只更新当前最终链接，reused表示根编译复用，字节未变不更新产物。

返回Vec按请求顺序保存build::Output::Object(Arc<ObjectCompilation>)或Output::Entry(Compilation)。原artifact、reused及程序／模板继续交给真实消费者；链接不读取B源码或调用编译器。每个对象单元使用调用方分配的独立输出，多个不同入口／目标可以共享BLBI输出目录；本接口不自动拆分所有源码导入，也不合并重复输出请求。普通模块、应用与算法库对象沿同一提供者集合消费COFF接口，算法库的常量／类型接口已用于真实CPU／GPU生成；无需Frame的已编译普通CPU函数可由算法逐元素范围调用，代码沿原COFF和静态链接交接，不能把它当作源码内联；已编译模块的资源工厂与算法根共同进入原静态目录，实际Frame、F及代码生命周期沿同一原生运行库交接。原始CPU入口映像可沿原选择及Allocation装入已编译COFF代码；根与导入模块的同源资源目录随同一最终映像交接，图Entry直接借用原产物完成链接；原始GPU映像的COFF调用、完整模块DLL及更广泛的映像组合仍按各自Issues推进。

图在活动Compiler许可内先固定实际.h／.b及解析声明，提供者公开接口和其类型导入也进入同一输入。后续任务不再次读取／解析这些文件，已捕获的可选.b缺失保持为本次事实，后续编辑或创建不混入这次构建。命中与失效仍由原正式COFF／BLBI身份决定，不把整份图或不被装入的私有实现变成调用者输入；固定解析输入只属于本次调用，不是跨次IR缓存。SDK／受管工具链安装的一致性沿原生产者，任意并发改写Rust SDK或外部工具的固定输入与分发仍未交付。

依赖满足即可继续后继，独立慢任务不建立整批屏障。所有实际编译与内层并行沿同一Compiler预算；等待图工作的线程不占CPU许可，选定CPU继续约束真实Cargo／rustc子工作。错误后不领取新的图工作，已开始消费者完成并汇合后才返回；不存在重试、后台重放或另一条编译路径。图及输入随本次调用结束释放，最终产物不持有编译会话。

本节交付显式SDK构建图与B源码固定输入。BLEngine现有消费者的批量组织与实际采用、完整4K启动性能、细粒度依赖／泛型及内联分发、多个模块DLL、域编译／域运算保持各自未完成状态，见双台账。

## 原生目标文件的增量复用（C-021.5.1b.1）

现有bc -c应用、--module普通模块及--program算法库直接复用输出的正式COFF目标文件。输入未变时输出末尾标明(reused)，原文件的内容与修改时间保持；不再做B检查、优化或CPU／SPIR-V发射。原始源码及已编译接口仍由原装入器读取一次，用于确定本次真实输入；没有常驻编译器、IR缓存或旁车清单。

SDK使用blanguage::build::compile_object，或在同一Compiler上调用build_object(path,root,dependencies,kind,output)。kind为build::ObjectKind::Module(&instances)、Application(entry)或Library { entry,prefix }。返回ObjectCompilation的object是可交给原对象／归档／链接API的已编译对象，artifact是实际COFF路径，reused表示本次是否复用。自由编译API继续无条件产生编译结果；需要跨调用统一CPU预算时由调用者复用同一Compiler。

生产身份包含实际.h／.b闭包、大小写敏感名称、具体类型／const导出选择及编译器生产身份。当前根独自发布完整.h，同名依赖不代替它；源闭包保留接口、模块／import和必要函数声明，实现正文按实际消费交接。声明身份使用原AST字段而不包含正文、注释或源码位置；函数类别、C ABI、泛型／const、参数／结果、访问、目标及合同保持。原实例生产者向同一本次输入集合记录普通、C ABI及cooperative实现的原key，摘要沿实际Source完整Span和推导参与要求形成。对象／入口各自消费自己的实现集合：改变已使用定义使消费者重编，未消费实现变化保持字节与修改时间；条件选择不同被调实现的具体实例分别保持依赖。同一模板实现本身改变时，其实际使用者均须重编，不把本项写成逐表达式增量或完整模板分发。

必要声明集合仍决定原声明检查，新增／改变未调用声明不能靠旧产物跳过；所有常量、记录／集合和有限布局、外部C签名继续按原职责处理，开放正文仍只在实际实例处检查。模块拥有其所有公开声明的推导参与合同，包括未交付机器码的开放模板；合同改变需要更新该模块，独立入口不附带无关模块的导出合同。实际嵌入的源码实现改变使消费者重编；普通已编译外调的私有实现变化且公开声明／合同保持时，调用对象保持并直接重链。

已编译导入头继续按原模块／import前缀与真正消费的Object符号处理；推导合同、参与要求及私有未生成函数的消费不依赖机器重定位收集。常量、记录、类型集合、外部C实例及实际名称查询继续记录原类别、key、name／module／imports，沿Declarations::resolve／resolve_type_set、原Source Span及传递依赖重算。限定名、本地优先、可见性、歧义、无绑定、变体父类型与family_members保持；无关未消费声明／变体或名称变化可复用。execution::infer、World及原并行汇合使用同一本次输入集合，不复制值、布局、候选、绑定规则或运行权威。

资源对象还包含其原生生产者实际SDK源／manifest、配置和环境输入；运行框架的native摘要覆盖源码build.rs及包消费build-sdk.rs的正文和有无，实际Cargo入口仍只由包清单选择。脚本变化须使已有对象失效，重新生产失败时交回原Cargo错误并保留原对象，不以旧对象命中冒称当前输入成功；恢复合法输入后沿同一构建入口正常生产。无附属运行框架的普通模块不增加该SDK依赖。源码SDK以受管Rust工具链生产附属代码。SDK根是原生子进程工作目录，生成单元只拥有生成源、stdio和输出。缓存假设SDK安装所有者保持工具链版本一致；任意外部包装器或二进制原地改写的身份发现尚未交付。该生产身份不改变模块ABI，也不声称整个SDK散列是ABI版本。

构建输入统一采用BLBO12，分别保存在COFF可丢弃的.blbuild节与BLBI1原identity字段中。source为实际构建请求、工作目录、编译器生产身份、有效参数容量、实际来源及其覆盖范围／摘要的结构化记录；实际导入及附属本机代码摘要、本次消费的声明／实现类别及键、名称查询仍由同一记录持有。不保存待编译正文、常量值、布局或IR，也不维护第二依赖清单。工作目录只保留原生产上下文，不参与代码复用判等；磁盘来源沿原请求的绝对路径与已解析模块根交接，原.h／.b配对及文件别名语义保持，嵌入接口标记保持。先比较实际请求、编译器生产身份、有效参数容量、来源及公开参与合同，再核对原实际导入、声明／实现及本机代码；仅切换启动目录可复用原产物，复用不改写记录中的原工作目录；没有当前构建输入协议或输入变化时正常编译新产物。真正COFF／BLBI包络及已识别记录的读取／解码错误直接报告，不因错误重新编译。构建输入记录与对象／入口映像／资源包络分别编码，对象与资源的现行兼容身份统一见[已编译模块的资源ABI](#已编译模块的资源abi)；构建输入编码不成为运行ABI或链接职责，既有可装入映像不因该内部输入记录改变而失去真实代码。链接仍可在没有B源码、Cargo和rustc时消费正式对象／归档；纯类型.h不需要空.b。源码依赖头与已编译导入头沿同一实际声明／名称查询决定失效，未消费记录的合法变化可复用原产物；实际消费变化继续重编，非法声明在命中前拒绝。当前根模块自己的完整导出接口仍属于其输入。

### 实际构建输入查询

`bc inputs <compiled.obj|entry.blbi>`只读取产物内的原构建输入并向标准输出交接JSON，不创建Compiler、读取B源文件、装入／执行代码或调用编译后端。它不接收编译资源选项，也不重新构建；读取或编码错误直接失败，没有当前结构化记录时不能从摘要恢复原输入。Rust SDK由`blanguage::build::input_facts(path)`交接同一记录的事实，不查询当前源码补填。CLI的artifact_sha256是本次实际读取的整个产物的SHA-256。SDK的`blanguage::build::input_facts_with_sha256(path)`同时返回构建输入和这一摘要。该摘要与同一COFF或BLBI的代码／范围查询一致；同一对象中的具体实例仍沿原精确符号及物理位置区分。

`blanguage::build::load_with_sha256(path)`在原入口映像装入读取中交回`(Compilation, String)`。摘要覆盖产生这份`Compilation`的完整BLBI字节，可以与同一次实际产物的构建输入、代码和源码范围查询关联。调用方将原映像交给现有CPU／GPU执行者，另持该摘要；文件名、源码名和运行地址各按自身的原事实解释。原`load`保留无需摘要的入口，两个入口使用同一映像解码与可信输入合同。

`Compiler::build_object_observed`沿同一对象构建入口绑定一份观察回调。原普通函数借用求解实际开始或返回时，`compiler::BorrowSolveEvent`借用参与的具体函数名、已读取的源码及半开字节范围。回调在原工作者同步执行，源码借用止于本次回调；并行工作可并行通知。`Finished { succeeded }`只交接原求解返回，构建调用继续交付实际对象或诊断。复用已有对象及求解前失败不产生求解事件。

查询包含原入口／接口路径、模块根、原工作目录、模块名、compiler生产身份、max_parameter_bytes及实际选择：application的entry、module的显式instances、algorithm_library的entry／prefix、entry_image的entry／CPU或GPU target。未在请求中指定的目标不由查询猜测。路径保留Rust原OsStr编码字节及可无损表示的UTF-8文本，不能表示时text为null而原encoded_bytes仍完整；来源名称可以是相对路径或嵌入接口标记，不能一律假定为当前目录下的文件。

每个sources条目交接原模块／来源路径、scope和fingerprint。full_source覆盖原完整源码；preamble覆盖模块前部；preamble_and_function_signatures覆盖前部与原必要函数声明，不含实现正文。imports_fingerprint交接原实际导入与声明／实现引用的整体摘要，native_fingerprint交接原附属本机代码摘要，没有该类附属输入时为null；declarations保留原消费类别／键与名称查询的module／imports上下文。实际使用正文仍由该原引用集合及摘要参与复用，不能把每个sources.fingerprint解释为整文件内容哈希。公开参与要求来自原模块推导合同，不能作为设备选择或原生运行结果。Rust的InputFacts直接交接原记录，source字段为上述请求／来源，imports／native为原摘要，declarations为原Declaration数据；不另建查询清单。完整源码内容、IR、汇编、运行内存和GPU现场分别由其对应能力交付，不由此查询冒称完成。

显式依赖图与本次B输入固定、实际泛型实例／嵌入实现粒度的失效已实现；完整模板分发与选择性内联策略、多个B模块DLL装载／热更新、域编译／域运算及新能力的BLEngine采用分别保持原范围。实现、原生CPU／GPU结果、提交与Git发布在双台账分别记录。

## 原生入口的正式产物复用
原`compile_entry`仍返回已检查IR。需要跨启动复用最终代码时使用：

```rust
let mut compiled = blanguage::build::compile_entry(
    path, module_root, "run<65>", blanguage::ast::Target::Gpu,
    std::path::Path::new("target/blink"),
)?;
let program = device.load(compiled.programs.remove(0).into_gpu()?)?;
```

编译后CPU模块缓存入口接收本次实际输入，不需要提供者h／b：

```rust
let input = blanguage::object::Input::read(module_or_archive)?;
let mut compiled = compiler.build_entry_with_inputs(
    path, module_root, "dispatch", blanguage::ast::Target::Cpu,
    output, vec![input],
)?;
let program = blcompute::cpu::CpuProgram::from_image(
    compiled.programs.remove(0).into_cpu()?,
)?;
```

CPU使用`Target::Cpu`、`into_cpu()`及原`CpuProgram::from_image`；模板保留在`templates`，GPU继续交给`load_template`，不会作为无前置条件入口。`Program`／`Template`是持有最终映像的原`resource::Image`类型，接口仍为ExecutionPlan／ProgramInterface；不构造空IR或在装入时调用后端。

生成目录由调用方明确指定。artifact为BLBI1文件，reused表示本次是否复用了根编译结果；请求中的路径、模块根、大小写敏感入口及实际CPU／GPU后端区分位置。源码路径与模块根按原生路径编码参与请求键，不因Windows路径含非Unicode代码单元而拒绝或有损替换；源文本和入口名称仍按原UTF-8语言规则处理。入口与COFF共用原World的声明／实际实现依赖记录，编译器生产身份、必要接口／声明、实际定义及目标决定是否重编，mtime和文件大小不充当身份。未消费实现改变可复用，实际依赖、可选实现有无或导入改变沿真实输入重新生成。产物不存在时正常编译，已存在但读取／解码错误直接报告，不隐式重编。

可信编译产物可通过`unsafe blanguage::build::load(path)`脱离源码读取，再交给原CPU／GPU装入入口；调用方必须保证文件确由可信编译器生成、代码和合同完整未修改。原机器码、SPIR-V、布局、依赖及前缀合同保持。缓存替换不改变已经装入的代码版本和真实资源所有者。

BLEngine算法准备、compute与simulate消费入口复用API，映像位于其target/blink。COFF复用、显式多单元图及实际定义粒度的泛型／嵌入实现失效已提供，消费者按真实准备依赖共享Compiler；本轮新能力的引擎采用另行记录。完整泛型模板分发、选择性内联策略、可替换多模块DLL、域编译／运行与热更新仍待实现。缓存命中不代表首次冷编译或完整引擎启动达标。

## 构建记录与可部署执行映像

BLBI1包络依次保存identity构建输入记录和image执行映像两段长度前缀字节。共享解码器借用本次文件缓冲，完整校验两段范围和末端；input_facts只借identity解出原InputFacts，不物化image。`blanguage::build::execution_image(path) -> build::Result<Vec<u8>>`只将原image复制成调用者独立拥有的Vec，不物化identity、不解析其内部版本，不读取B源码、不创建Compiler或设备、不重新编译／重编码代码，也不修改原BLBI。入口复用与可信load使用同一包络解码和实际字节借用期；构建缓存继续保存完整原输入身份，按原规则命中或失效，原编码、错误及原子写出保持。

```powershell
bc image target/cache/entry.blbi -o target/deployment.blri
```

CLI在Compiler初始化之前处理，读取原映像后由OS创建新的输出文件；已有构建、缓存或部署文件保持原所有者，输出已存在、父目录不可用、真实读取／解码／写入错误均直接报告，失败不输出成功JSON。成功结果kind=execution_image_export，source_path／output_path及其原生编码字段保存本次实际路径参数，byte_length为原image的完整字节数；成功导出不等于装入、设备支持、实际执行或安装。扩展名只是示例，恢复以原BLRI编码及兼容身份为准，不按文件名猜格式，不接受把原始BLRI再当BLBI导入。

部署文件逐字节等于原image，包含原resource::Module的类型与CPU机器码／GPU SPIR-V、实际执行布局／顺序、必要重定位、原生展开和访问／前缀合同；不包含原identity的源码位置、工作目录和构建输入清单。原映像中运行所需的程序／类型名及系统库引用保持，不能把这些合同当调试材料删除。本入口不做符号裁剪、Shader改写或正式安装，可选观察材料的保存／装入仍沿C-021.7.9.5.2.2，平台符号选择沿原.63／.64。

Rust消费者可关闭blanguage／blcompute默认compiler功能，直接读取部署字节并调用原`unsafe resource::from_compiler_output(&bytes)`，随后把CPU／GPU映像交给原CpuTemplate::from_image／Device::load_template。原Module按TemplateProgram标记恢复，原DomainBindings继续消费真实前提及完成关系，不无条件转换为ExecutableProgram，不因移除构建记录而允许不相关数据取得prefix等合同。字节信任与实际执行权限仍由原装入边界承担；不新增Frame、B解释器、宿主转发、资源拥有表或另一运行库。输出字节／文件归调用者，代码、存储及本机／GPU资源继续由原所有者保持到最后真实消费者结束。

## 开放普通值类型形参
记录、普通／cooperative 函数和当前 计算函数 可写不带集合约束的 `<T>`。T 在每次具体使用处绑定普通值类型：支持 bool、数值、矩阵、数组、用户记录及其合法组合，不要求把用户类型加入库的有限清单。例如 `.h`：

```c
struct Packet<T> { u32 tag; T payload; };
Packet<T> packet<T>(T value, u32 tag) on compute;
void assign<T>(write T target, T value) on compute;
```

对应 `.b`：

```c
Packet<T> packet<T>(T value, u32 tag) on compute {
    return Packet<T>(tag, value);
}
void assign<T>(write T target, T value) on compute { target = value; }
```

`packet(UserRecord(...), 7)` 从实参推导 T，返回名义类型 Packet<UserRecord>；也可显式指定类型。相同 T 的所有推导必须一致，布局相同的不同名义类型不会合并。数组、名义记录和 dataset／tile／view 元素沿原形状与类型关系推导；开放类型可以和有限集合、有限／开放 const 参数组合，例如 `<T, I: scalar_integer, const N: u32>`。类型参数仍在 const 参数之前，不能只从预期返回类型、运行期容量或 Count 猜测它。

开放T可实例化为合法view函数值，仍不擦除dataset、tile、texture2d／texture3d、acceleration或view包装的权限与寿命；`view<T>`／`dataset<T>` 等仍通过原显式接口声明引用或资源访问。普通记录内部合法的 view 字段继续遵守原引用图及目标限制，开放类型不授予构造、写出、传输或逃逸权限。void 不是数据类型。write／inout、完整初始化、借用重叠、模块可见性、设备、索引和返回规则在实际实例上保持。

完全有限的泛型声明仍枚举并检查全部允许实例。只要声明含开放类型或开放 const，就不能枚举其所有替换；依赖类型／值的布局和正文在实际使用时完整检查，未使用声明仅检查原通用签名并列入 Compilation.open_generics，CLI 输出 `open generic declarations (body checked on instantiation)`。检查未调用的开放库不证明所有类型都能执行其正文；错误仍定位到实际具体类型和源码。只有开放 计算函数 而未选择具体入口时，build 报 B240；具体入口沿原 `compile_entry` 或 `bc -c --program` 语法选择，例如 `copy<UserRecord,65>`。

开放类型用于现有静态实例化和集合函数引用，不新增类型擦除、运行期分派、宿主逐元素回调、额外 uniform、布局表或新 IR／ABI。CPU普通原生调用及独立模块已有前述有限实现；通用约束接口、关联类型、符号泛型体检查与一般模块／资源契约仍未完成；统一函数／结果模型按架构设计继续推进，当前语法范围不构成永久函数分类。

## 原生缓冲区与AS异步交接

upload／upload_texture、GPU zeroed及BLAS／TLAS构建返回已提交的真实资源，其生产／最后使用点沿既有Execution传递；调用者不从CPU帧或返回时刻推断设备完成。后续构建／计算按实际资源建立同队列内存访问关系，命令、暂存、scratch、实例页与引用在真实使用结束后释放。top_level_device消费GpuDataset所有权，直接读取原GPU矩阵页；BottomLevel／Acceleration的完成接口只观察本次构建，Texture的完成接口只观察本次图像生产。详细原生API与范围见[设备实例](DeviceInstances.md)。下载、显式等待、当前draw及呈现仍可阻塞；直接跨设备传输、多物理 GPU 选择／验证、多队列、refit和完整流送未据此完成。

## 整程序编译映像与原生装载
编译器拥有从已检查 IR 到全部机器入口的转换，执行器拥有代码装载、存储、提交和实际完成后的释放。`native::compile_program` 生成 CPU 的 `ProgramImage<P>`，`spirv::compile_program` 生成 GPU 的 `ProgramImage<P>`；P 保留 ExecutableProgram／TemplateProgram 的入口种类，但不保存原 IR 对象。映像包含最终代码及 ExecutionPlan，后者只保留接口、域关系、访问依赖、集合布局、工作组／共享存储和实际设备需求；自定义集合的投影、单位元、合并器及普通值指令均已编译，不进入执行布局。

下例是 Rust SDK 对一个 on compute 入口的构建与装载，用于现有原生宿主：

```rust
let source = blanguage::compile_entry(path, module_root, "run")?.programs.remove(0);
let cpu_image = blanguage::native::compile_program(source.clone())?;
let gpu_image = blanguage::spirv::compile_program(source)?;
let device = blcompute::vulkan::Device::for_program(gpu_image.program())?;
let cpu = blcompute::cpu::CpuProgram::from_image(cpu_image)?;
let gpu = device.load(gpu_image)?;
```

普通入口使用CpuProgram::from_image／Device::load，保留终端消费的原Result；条件入口使用CpuTemplate::from_image／Device::load_template，装入失败沿Err((error,image))交回原错误及同一未改映像，成功才取得原执行计划，仍须交付真实DomainBindings。GPU准备阶段的部分管线／布局由原Compiled持有并在错误时释放；持有返还输入的调用者可显式再次装入，终端消费者直接释放输入并传播原错误，不自动重放。默认compiler feature提供CpuProgram::new、CpuTemplate::new及ffi::LibraryProgram::new，沿原整程序编译及装载工作。GPU调用方先经Compiler::spirv或spirv::compile_program形成完整映像，再把同一映像的ExecutionPlan交给Device创建并load／load_template；设备只消费已编译结果。关闭 compiler 后，CPU／GPU 只保留映像表示和原装入入口，不编入 native/codegen.rs、spirv/codegen.rs 及其发射模块；公共 CPU 映像与范围 ABI 定义统一在 native/image.rs，GPU 映像、PAGE_BYTES／BINDING_BYTES 页绑定及 PREFIX_LEVELS 前缀布局定义统一在 spirv/image.rs。Compiler／Processor、ReductionKernel及CustomReduction::kernel仅在compiler角色提供；关闭compiler时不编入优化器或编译许可预算，CustomScan／CustomSegmentedReduction的运行摘要与真实布局保持。IR Program的ProgramInterface实现、CustomReduction到ReductionLayout转换及ExecutionPlan::from_program只在compiler角色提供；执行器消费原ProgramLayout／ExecutionPlan及其ProgramInterface查询，不再次扫描IR生成设备要求或阶段布局。ExecutionStage::input_words()借用本执行计划原inputs集合，借用随同一计划存活，不创建另一份拥有集合。参数接收直接遍历这些输入字，核对非本阶段域的实际源数量能覆盖域数量，不为相同来源的多个字段另建去重集合；字段、数量、错误及执行完成保持。Ty::syntax与CFunctionType::syntax只在compiler角色把共有类型重建为源码TypeName，供原World、泛型与检查消费；运行角色保留实际布局、Access及签名／合同身份，不包含这两个源码转换入口。应用根与算法DLL的原archive::compile为执行框架生成独立Cargo单元，对blanguage／blcompute依赖明确关闭默认特性；根包装只消费共同资源ABI、原类型／执行布局与已编译映像。发行包交接工具及源码SDK，不将workspace编译器特性统一后的静态库当作无compiler运行库采用。其余共有表示与完整运行库依赖分工仍沿Issues的C-032.1.3推进。CPU装入只借用原映像，向实际执行内存写入指令、展开字节、同VM函数表和重定位地址；源字节及原布局不被本机地址改写。Windows函数表成功登记后才交付代码入口，登记引用由原Executable持有；最后一个真实消费者后先撤销登记，再释放同一VM和原DLL引用。部分装入失败沿同一Executable／Library释放本次实际资源，CPU模板失败返回同一输入，阶段完整成功后才移动原plan。CpuProgram::from_image及从IR构造的new仍是按值输入的终端消费者，失败交回原错误并结束输入；CpuTemplate::from_image失败返回(error,image)，调用者按真实拥有需要接回输入或显式结束其消费。GPU装入失败及实际运行失败的完整输入／部分效果合同仍见Issues C-020.5.2，不能由CPU模板本项保证替代。CPU 装载保留真实 DLL 重定位和代码消费者的模块所有权；GPU 装载按编译布局建立原管线与资源，设备能力错误直接报告。

ProgramInterface 是编译结果与已装载程序共同提供的只读接口。Dataset 创建、不可变 view、域关系和引擎数据打包消费该接口；已装载 CPU／GPU 程序的 program() 返回 ExecutionPlan，不再为了旧类型签名保留整份 IR。编译器内部的 ir::Program 继续拥有指令和优化。生成 DLL 使用同版本编译器的可信 ProgramImage::from_compiler_output 恢复最终代码及其布局；该 unsafe 边界不能接收宿主输入、改写代码或伪造条件模板的前缀关系，原整份 IR 恢复入口已删除。

bc build、算法目标文件编译和 SDK 使用同一份程序映像；阶段导出沿用原文件名及布局描述，不重新挑选或发射辅助代码。值 IR 和集合函数正文在编译侧结束，物理数据及完成点仍由既有执行所有者持有。这里没有新增 B 应用的动态数据、CPU／GPU 调用语法或隐式数据传输；这些继续属于 C-011，不能用 Rust SDK 或算法 DLL 的完成代替。

## Windows x64 DLL 与 C/C++／Rust 入口
```powershell
.\target\release\bc.exe -c `
    sample\lessons\map.b `
    -o target\interop\map.obj `
    -I sample `
    --program scale
.\target\release\blink.exe `
    target\interop\map.obj `
    -o target\interop\map.dll
```

`bc -c ... --program name` 接收一个公开、具体且无 requires 前置条件的 计算函数；泛型使用既有 compile_entry 的具体入口语法。编译输出 .obj，再由 `blink input.obj -o output.dll` 输出 .dll、.lib、.abi.h 和 .rs。B 的 .h 仍是语言接口，.abi.h 是供 C/C++ 编译器包含的头文件。接口字段布局来自原 Ty，dataset 传指针与完整记录元素数，uniform 传生成类型的只读指针。当前接受不含 view 的纯值类型、向量／矩阵、记录和固定内联数组；纹理、AS 与条件模板明确拒绝。内部集合、阶段和 prefix 仍由 B 编译器检查并执行。

C ABI create／run 返回 0 或 -1，错误缓冲区容量包含结尾NUL：容量0不写入，允许NULL；容量1只写NUL；这两种没有文本载荷空间的调用不格式化错误。更大容量按完整UTF-8字符截断并以NUL结束，成功写空文本，原错误由本次状态调用消费并释放；destroy释放DLL自己创建的handle。run 为同步、包含数据交付的完整调用。算法DLL纯值准入直接采用原Ty的拥有资源和view查询：固定零长度数组不引入元素资源，变体及Result按各可能载荷判断。实际拥有资源或可能含view的完整类型继续使用原生Rust资源绑定。有载荷的dataset以指针和u32完整记录数交接，零尺寸元素dataset只传u32数量，仍参与原CPU／GPU逻辑范围及执行；零尺寸uniform不产生机器参数，嵌套零尺寸字段不生成载荷成员。C/C++头文件、转发及Rust绑定共用这份实际参数表，须与DLL配套重新生成。C++使用RAII；Rust绑定对有载荷数据提供切片，对零尺寸数据直接接收数量，并保持Result、Drop与创建线程所有权。Rust 仍可直接调用原有 compile_entry、CpuProgram、Device、GpuProgram 和所有权 API。B 调用宿主 DLL 使用前述 CPU extern C 声明，不改变这里由宿主持有的算法入口与资源。详见 [原生集成](NativeInterop.md)。

### Rust Dataset的单标量字段写入

`Dataset::set_scalar(index,path,Value)`按当前Dataset的Ty写入一条记录中的单标量ABI字段，支持Value当前定义的各标量类型并要求精确匹配。路径与field_offset共用声明来源，嵌套变体还消费当前记录的实际活动判别字；不能通过写未激活载荷冒充变体构造。记录下标、字段及标量类型必须匹配，失败交接真实错误并保持存储，宽值写完整8字节，其他字段与填充保持。

此接口不复制布局、增加资源权限或管理引擎参数含义。原Ty的字段路径与view遍历按实际tag标量共用判别值解释：Result的Bool判别字零选择Ok、任意非零选择Err，带载荷enum按声明的u32编号精确选择；查询和定点写入不改写原tag字节。定点写入只消费原Ty选中的活动单标量字段，不取得任意可写payload；已有view描述、绑定及其原来源保持，由最后真实消费者结束其寿命。words_mut交出原始可写存储时仍撤回旧view图，真实图重绑定继续沿原with_views交接实际来源。uniform参数继续沿同一个Value表示按逻辑分量传入，不能用Value数量推算物理存储字数。

## 同索引独占多输出
一个parallel以主数据集定义invocation域，同一个invocation可以读写其他普通可写数据集的当前index()元素。各根的read／write／inout权限保持，不授予其他元素的散写权；固定同形容量提供当前索引的范围事实，包含空域，但不会把不同资源判为同一资源。动态容量按已证明的范围条件访问。

```c
struct A {f32 value;};
struct B {u32 value;};
void split<const N:u32>(write dataset<A,N> first,write dataset<B,N> second) on compute {
    parallel(item in first) {
        item.value=f32(index());
        second[index()].value=index();
    }
}
```

每份inout当前元素均由调用者提供初值；write字段在读取或inout借用前必须初始化。实际写入的普通write输出须在全部继续路径初始化所需字段，未完成报B108，未初始化读取／借用报B142；bounded append继续沿原有效前缀语义交付。非当前元素仍无普通写权；同一阶段对同一存储的普通跨元素读取与写入报B166，未证明范围报B137，read／不可变view不能获得写权。子程序可写数据集别名沿用B118，当前位置借用仍要求互不重叠。

同一阶段中，append的输出与单u32计数不能再借作普通write／inout当前元素，也不能与inout tile_input域重叠，反向顺序同样拒绝为B221；计算append值时取得的借用也参与判定。这扩展原append的共享写入与独占元素互斥，不产生运行时检查。同一输出／计数对的连续append保持合法。

普通map、where的原索引以及同域阶段组合复用同一IR Store／读取写入集合、现有CPU／SPIR-V后端和执行完成点，不增加GPU复制、宿主逐元素回调、运行期下标修正或额外调度器。实际使用是BLEngine在一次主命中中同时生成像素与48字节可见性记录；可选输出在const分支关闭时没有对应Store，普通图像不因此获得全图历史。


## 原生纹理的逐轴寻址
二维／三维采样资源显式携带每轴 `TextureAddress::Clamp` 或 `TextureAddress::Repeat`。`Texture2D::new/read` 与 `Texture3D::new/read` 默认创建 Clamp 视图；`image.with_address(u,v)`／`with_address(u,v,w)` 消费该资源视图并共享原不可变像素，`address()` 返回当前模式。资源实际身份随匹配维度的只读参数或资源数组交接，不由材质函数复制图像。相同像素的不同 CPU 视图不等于已经共享 Vulkan 图像存储；当前上传各自创建原物理资源。

CPU 对归一化坐标执行逐轴 texel 寻址，二维四个 tap、三维八个 tap 参与层内及 mip 间过滤。Repeat 保留负坐标、整数周期及接缝两侧的贡献，不能只对坐标取小数后继续钳制 tap。24 字节 CPU 纹理描述符的第二个 u32 保存逐轴 repeat 位；每级描述为 24 字节，width／height／depth 位于 0／4／8，实际像素指针位于 16。二维 depth 为 1，采样维度由原 TextureBinding 交接。原生降低直接读取原描述并执行 64 位寻址，不调用宿主采样函数。Vulkan 设置同一资源的 U／V／W addressMode，SPIR-V 直接发出相应维度的图像采样指令。

`vulkan::Texture::address` 返回原上传模式，`download_texture` 的独立 CPU 快照保留该模式，后续显式上传另一个逻辑 Device 继续消费它。`.bltex` 只保存实际格式、像素与 mip；采样模式由实际资源／材质声明，不改写图像文件或改变旧资产解释。CPU／GPU 的采样精度与允许差异统一见[采样数值与目标精度](#采样数值与目标精度)。当前仅 Clamp／Repeat，镜像、边框色、各向异性、独立 sampler 类型及压缩格式仍未实现。


## 原生 texel 格式与存储所有权
`TextureMip::pixels` 为 `TextureTexels::{Rgba32Float(Vec<[f32;4]>),Rgba8Unorm(Vec<[u8;4]>),Rgba8Srgb(Vec<[u8;4]>),R8Unorm(Vec<u8>),Rg16Unorm(Vec<[u16;2]>),Rgb10A2Unorm(Vec<u32>),R11G11B10Float(Vec<u32>)}`。原 float Vec 可通过 `.into()` 交接；`Texture2D::format`／`Texture3D::format` 从各自实际负载取得格式，整链同格式。`TextureTexels::as_bytes/from_bytes` 只交接真实字节，不维护解码后的 CPU float 镜像。二维／三维共用原生 CPU 纹理描述，含 mip 数、逐轴地址模式、mip 指针与格式（offset 0／4／8／16，stride24）；每级描述的宽高深及指针布局见上节。采样直接生成相应字节／dword 装载、UNORM 或分段 sRGB 运算，再完成对应维度的层内过滤及 mip 混合；格式来自所选资源，混合格式数组不要求统一转换，也不经宿主 texel 回调。

Vulkan 按真实格式创建 `R32G32B32A32_SFLOAT`、`R8G8B8A8_UNORM`、`R8G8B8A8_SRGB`、`R8_UNORM`、`R16G16_UNORM` 或 `A2B10G10R10_UNORM_PACK32` 图像／视图；格式不支持时明确报告，不能静默替换。上传／读回按真实 texel 字节数构造复制，每级 staging 起点满足4字节对齐，尤其保留R8奇数尾层；文件不保存这些平台填充。复制区域布局见 [VkBufferImageCopy](https://docs.vulkan.org/refpages/latest/refpages/source/VkBufferImageCopy.html)，对齐约束见 [vkCmdCopyBufferToImage](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdCopyBufferToImage.html)。快照保留实际格式与地址模式，逐字节保留原负载，包括 float NaN 载荷、负零和次正规位，不通过浮点算术重建。资源仍沿原上传及最后采样完成点持有，不新增长驻状态、上传副本或全局资源表。

实际格式往返、采样误差、图像分配及消费者验证结果只在双台账记录。CPU软件采样与硬件过滤的舍入精度不同，不承诺逐位一致或天然材质视觉等价；压缩格式、流送与完整产品性能仍按真实缺口推进。


## 集合阶段的字段访问与独占输入
普通数值及用户值的归约、扫描、分段扫描／归约都可在阶段期间只读借用 `read` 或 `inout` 数据中的实际源字段。原子参数不转成普通读取，`write` 参数不授予读取；输出仍需原可写权限。一个参数为inout不表示每个阶段都必须写整份数据，也不要求复制成另一份read镜像。

同一数据集的源和目标可以是互不重叠的字段，例如 `scan_exclusive_map(field.count,field.begin,project,identity,combine)`。检查沿已有Ty分量的真实字节偏移进行；目标不能覆盖源的任一逻辑分量，分段形式也不能覆盖实际段头。记录整体和它的成员存在真实交集，不能当作两个独立字段；填充未被算法读取／写入时不冒充有效分量。仍不支持源值自身的原地覆盖扫描。输入Count、独立输出Count、容量和资源所有权继续遵守原边界；不同参数名称不证明底层存储独立，也不改变跨原生接口的可写别名规则。

CPU复用原原生阶段和真实指针；GPU根遍历仍读取原源字段，递归和传播的摘要由其实际物理来源交付。当源／目标是同一逻辑binding时，原公共槽无法同时表示原记录与另一份摘要，所以私有执行布局为这些扫描提供一个实际扫描源角色，编译器及执行器使用同一槽位。它只是既有摘要的原生绑定，不分配输入镜像、增加扫描阶段、宿主计数或运行期别名状态；不含此关系的程序保持原私有槽需求。分段摘要、投影类型、顺序、GPU有效计数和最后消费者释放保持。

Release原生消费者覆盖0、1、65、4099、1000003条记录的普通与映射扫描／归约、分段扫描及分段归约；另有同记录单元素归约及compact实际生成0／1／65／100003计数的prefix后续消费，共25组CPU／GPU完整记录与独立u32／u64参考相同，未改字段、填充和尾部保持。六份分别编译的真实重叠、write／atomic输入、段头覆盖与Count覆盖仍报B170。65条记录暴露的递归物理槽覆盖，以及分段传播遗漏均按真实根因修正，原失败保留。原始记录E:/BLEngineMeasurements/DirectionalEnvironment；这些结果不代表完整语言、渲染、视觉或通用效率目标达标。


## CPU应用中的dataset不可变借用

`view<T>(rows)`借用一个现有、已完整初始化且实际位于CPU的`dataset<T>`位置，也可借用拥有记录／固定数组中的实际dataset，或复制同型的现有view。它取得原存储的只读引用，`.length`、有界游标和普通索引沿原view规则；实际长度Proof进入原索引查询，既有Data／Tile／View严格索引界沿完整长度的精确身份进入原below_length，same_length不合并存储或权限；没有载荷复制、地址提取或隐式download。例如，接口.h声明同一函数，.b实现：

```b
view<Row> inspect(read dataset<Row> rows) on cpu {
    return view<Row>(rows);
}
u32 total(view<Row> rows) on cpu {
    u32 sum = 0;
    for (row in rows) { sum += row.value; }
    return sum;
}
```

`view<T>(source,start,end)`借用同型CPU dataset或view的半开元素区间`[start,end)`。start／end为u32，必须沿边界值的已有上界／偏移及控制条件证明`start<=end<=source.length`，否则报B137；合法空区间与末尾空区间保留。地址按source实际元素步长乘start，以64位寻址偏移，长度为end-start，不复制载荷、不钳制或新增运行时范围检查。source、start、end按原顺序各求值一次；后续实参不能写入、移动或传输已经借用的来源。无本层seek的for(unit in first..source.length)保留原捕获first<=unit，可在循环体构造view<T>(source,first,unit)；首轮展开、continue和嵌套循环不改变原起点，后续改写起点变量不改变已捕获值。本层seek可重入更小位置，不能沿用初始下界。区间保留原view全部来源及寿命，缩小区间不获得底层写权；普通记录、固定数组、泛型、递归及write／inout输出沿同一合同。CPU构造仍要求实际CPU存储；跨记录字段的view保留父存储的真实步长，GPU存储须先显式download。

view及含view的普通记录／固定数组可在CPU函数间复制、参数传递与返回，也可通过write／inout普通值输出；开放类型实例沿实际元素类型组合。返回引用保留调用者真实存储的来源，独立B模块／静态库导出同一合同，不需要库实现源码。每个返回／written出口先在本出口的路径、值和初始化关系中改名，再参与合流；函数合同只命名真实输入与最终合流输出，不把私有出口别名作为其他出口的事实。分支选择或动态资源下标保留全部可能来源；不能以不同临时寄存器名推断存储不同。

活动借用期间，原dataset的实际存储不能被写入、替换、upload／download或先行释放；普通可写元素位置按实际子资源判定冲突，活动子view不阻止修改无关父字段；仅移动本机拥有元数据不改变实际存储与view来源，把拥有值交给其他函数仍须满足该交接的借用合同；记录／数组副本、游标、正在准备的实参及下标求值同样保持借用。view不能返回指向本函数将释放的局部dataset，也不能超出临时dataset实参或内层资源的实际寿命。借用存储并不取得所有权，普通C ABI仍不接收view。元素内部view的未来实际使用同样保持借用，已结束的消费者不延长载荷借用，仅查询dataset长度不引入元素内容借用，view本身的来源检查保持。将拥有子树借入write／inout参数时，其内部指向自身的view随资源交付给被调方；子树外的view以及未交付字段中的view仍保持调用者借用，部分字段交付不一并移除它们。普通inout可以在输出拥有者继续存在时返回其实际子dataset的view；最后实际view消费者完成之前，调用者不得替换相应子资源或移交／释放其父拥有者，未借用兄弟字段保持可写。投影实参按原记录字段路径对应实际子资源，嵌套dataset各自保持拥有层级。普通read参数返回内部存储view时，来源沿实参的实际字段模型交接，只读权限和原存储寿命不扩大；本函数局部且未交付的拥有者仍不能经返回view逃逸。

CPU应用根据控制流中的可能后续使用维持借用；最后一次实际使用之后，原所有者可以继续修改、移动或显式传输，无需仅为结束未再使用的view而增加局部块。记录字段与固定数组分量分别保留来源；覆盖时，右值已经完成的读取及覆盖位置随后对新内容的读取，不成为旧内容的后续消费者；仍存活的旧view和静态F保持真实借用。确定覆盖结束旧值的后续使用，动态覆盖不会清空其余候选元素的来源。循环同时保留首轮、回边和实际出口，write／inout输出、待交付的选择结果、实参／下标求值和自动释放同样遵守实际寿命。

这一实现不等同于任意运行时关联条件的完整推导。普通CPU自递归／互递归、泛型、记录／固定数组的view返回与write／inout值输出，已按实际函数输入、结果分量和真实回边联合推导来源。调用点只保留实际借用的输入；没有正常返回来源的纯递归环仍报B271，不能假定输出无借用。CPU自包含拥有view聚合见对应章节；持久dataset图的B构造、应用内GPU图及异步／远端关系仍是能力缺口。这些实现范围不构成永久语言限制。view机器表示为24字节、对齐8；循环入口引用在编译期闭合，不加入运行期或公共对象格式。


## CPU拥有聚合的read参数

普通CPU函数可直接声明`read Bundle value`或`read array<Bundle,N> values`，其中Bundle可以含dataset、拥有记录、固定拥有数组及一般变体／Result的活动资源。声明的类型、形状和布局必须一致；read不转移拥有权，调用后原绑定仍可继续使用、移动或进行显式传输。资源仍归原Frame，底层数据不会为此次调用复制。

同源read参数以及聚合与其内部dataset的只读参数可以重合；它们不能与同一次调用中相同资源或元数据位置的写入、独占借用或拥有交接重合。参数内部的普通字段和资源字段都不可修改，不能替换句柄、移动资源、上传／下载或把借用作为拥有值返回。对read变体使用match时，活动资源载荷继续只读借用；普通值载荷按原值规则使用。

read实参可以是已有拥有值、静态／动态选中的拥有聚合或完整临时值。下标和实参仍按原求值顺序执行，实际候选来源及临时拥有者持续到真实调用完成；返回view只能继续借用仍存活的输入。当前CPU递归／互递归、泛型、字段和数组来源规则同样适用，不允许从临时聚合借出越过其释放点的view。独立B对象／归档使用同一合同，链接不需要库.h／.b。

拥有聚合write／inout、动态拥有位置与CPU自包含拥有view聚合按相应章节规则支持；只读借用不获得这些修改权限。普通C资源ABI、拥有元素子资源的完整长度关系／批量增长、完整资源阶段、异步／远端和版本迁移仍待实现。

## view变体、Result与拥有值组合
view可作为enum／Result的函数值载荷，并与普通记录、固定数组、开放泛型及CPU拥有聚合嵌套。view保持对原存储的不可变借用；例如`struct Mixed { dataset<Row> owned; view<Row> borrowed; };`的owned归该聚合，borrowed仍借用外部dataset。返回或移动Mixed只交接其实际拥有资源，不能把本函数局部存储借出，也不能让外部view越过真实来源的释放点。

```b
enum Ref : u32 { Empty = 0; Some(view<Row>) = 1; };
result<view<Row>,u32> choose(read dataset<Row> rows,bool active) on cpu {
    if(active){return Ok(view<Row>(rows));}
    return Err(19);
}
```

构造、match、try、返回、字段／数组投影、循环和递归交接同一来源。拥有记录的各字段从左到右求值，先前view在后续字段求值时仍有效：`Packet(view<Row>(rows), mutate(rows), owned)`必须拒绝，不能因为view尚未绑定到最终Packet就允许修改来源。数值事实失效保留借用；最后使用后原所有者可继续写入或移动。

变体使用原判别字与重叠存储。某分支没有view是实际已知状态，与未知返回来源不同；纯Err／Empty可以形成合法来源合同，无正常来源的纯递归仍报B271。实际资源来源保存在原Data叶，Proof只记录类型中真实view的来源，资源输入按声明类型中的叶顺序命名，避免重叠句柄与view共享标量槽时混淆。

CPU普通函数与独立B对象／归档交付相同合同；compute／GPU普通函数也可在已有宿主只读图上组合这些view值，持久变体view存储由宿主按下一节绑定，普通值本身不拥有该图。编译合同交接必要的来源身份；view的24字节／对齐8布局、原Frame与最后CPU／GPU消费者保持。CPU自包含拥有view聚合及整份拥有聚合write／inout按对应章节支持；拥有元素子资源的完整长度关系／批量增长、持久dataset图的B构造、应用内GPU view构造、异步／远端和版本迁移继续开放。

## 持久dataset中的变体view
宿主现在可以按实际活动载荷，在不可变dataset中保存enum／Result的view及其记录、固定数组、间接view组合。源B通过原read dataset／view读取、match、普通函数和有界索引消费；仍不能写入view元数据、数值伪造地址或借此取得可写资源。例如：

```b
struct Row {u32 x;u32 y;};
enum Ref:u32 {Empty=0;Some(view<Row>)=7;Number(u32)=19;};
struct Node {array<Ref,2> entries;result<view<Row>,u32> answer;};
```

原Dataset::for_view通过声明路径取得某个分支的子元素类型。with_views只接收该记录真实活动路径，例如entries[0].Some或answer.Ok；空分支没有引用，不需要伪造占位源。数组下标、字段与分支名称共用原Ty布局，名称保持大小写；Some和其他分支即使复用相同字节，也不能互相替代实际活动路径。

CPU仍以freeze交还真实不可变所有权，并通过with_views绑定子资源。GPU可上传闭合CPU图，或以upload_with_views绑定已经属于该逻辑设备的GPU子资源；download重新绑定真实主机来源。父与子资源、共享来源及最后CPU／GPU消费者沿原所有者持有，传输不从物理地址反向推导拥有权。读取原始字节不等于拥有引用，words_mut会放弃原图；活动view须重新绑定，无view分支则无子资源可要求。

真实判别字决定每条记录的引用位置与数量；含多个view的分支、普通数值分支、负编号枚举及Root Result／数组遵守同一规则。bindings只记录实际字偏移和原source索引，CPU与GPU地址在相应目标的真实位置编码，跨页不改变声明布局。view的24字节／对齐8机器表示保持。当前宿主入口为原生Rust API；CPU自包含拥有聚合按对应章节支持。持久dataset图的B构造、普通C ABI／算法DLL的view图值接口、应用内GPU构造、可写view和异步／远端按各自未完成范围记录。

## 拥有聚合的write与inout

普通CPU函数可以声明write State或inout State，其中State可为拥有记录、固定数组、变体／result及其嵌套组合。参数直接引用调用者的实际位置；已有记录／数组的静态字段与合法动态选中聚合、泛型实例、递归调用和拥有临时值沿同一规则使用。此处管理的是聚合中的拥有内容，容器本身仍由调用者持有；普通dataset借用的既有访问语义保持。拥有记录／固定数组中dataset字段的长度沿父值当前数值分量投影；资源叶继续交接实际资源来源、初始化与拥有关系，不以合流前私有寄存器补造合流后的长度事实。

inout需要已初始化、未移动的全部活动拥有资源。函数可以修改普通字段与数据内容、显式传输、取走旧拥有值或替换整体／字段；若取走内容，则在每条正常返回边补回。旧值可以作为函数返回交给另一真实所有者。write允许未初始化的位置，也可覆盖已有值：所有实参与下标求值完成后，在本次调用前释放该位置原先仍拥有的内容。write入口没有旧值的读取或初始化权限，不能依赖调用者恰好已有值。原native_call的源码调用完成点只属于传入真实实参AST的原调用：实际IR调用与写回输出完成后，先推进该源码点，再结束参数临时拥有者。生成的静态F、dataset初始化及归约调用没有原调用AST，不因共用诊断span推进外层源码状态；各自实际调用、来源及资源释放继续原职责。源码完成点不由生成调用代替，函数退出及资源完成继续归原所有者。拥有值进入释放时，原入口沿实际拥有子树／位置及完整dataset拥有者取得该点仍留在caller的view借用；原递归只消费这份借用与本树尚未执行F的关系，资源叶不再重复查询caller的view借用，真实游标／位置借用仍沿原入口检查。借出dataset内容和结束整个dataset拥有者分别交接；整树F随实际拥有者进入原释放树，内容投影仍保留真实外部消费者。临时拥有位置按实际分量交接普通地址与F来源，不省略未交付字段。旧内容的F和资源仍由原释放路径结束，新输出写回同一位置，共同Data.elements不承载调用期间的局部未初始化状态；未交付字段F、跨元素F及已保存外部地址继续参与借用检查。运行时下标的固定拥有数组沿原Position::Indexed与Presence选择条件交接实际分量；调用者借用的原来源边只在未交付该分量的条件下保留，所选旧树继续原F与资源释放。不能将所有候选元素同时视为已交付，也不能省略未交付字段、跨元素F或已保存的外部地址。资源叶只消费已交付的实际释放树；原位置写回和作用域出口在原拥有者处结束对应状态；具名dataset在自身F及资源释放后结束这个绑定的Ownership，已结束的F不再保留清理借用，不按合流Data.root身份横向改写其他绑定，未选中元素保持原拥有与最后释放。

每条正常返回边都须完整初始化输出普通字段并归还活动拥有资源，返回Err同样如此；未形成输出的never路径没有正常结果。变体只要求实际活动载荷，普通记录／固定数组保留各自的完整初始化关系。临时实参只持有到实际调用完成及其真实消费者结束，不能把借用它的view作为可存活结果。

返回值和独占输出在同一个函数合同中交接。输出资源的CPU／GPU驻留、普通分量事实、资源初始化及view来源会代入实际调用位置；自递归、互递归与泛型保留同一来源关系。返回的view可以借用输出中仍存活的新拥有资源；借用当前函数即将释放的局部dataset、让临时来源逃逸或在活动view期间替换其资源仍拒绝。同次独占借用不能与可能重叠的位置或实际只读／原子借用重叠，只读聚合不能提升为可写。

普通extern C不接收整份拥有资源；B内部借用仍按原地址、Frame和真实完成规则交接。函数内动态位置、条件输入／输出及CPU自包含拥有view聚合见对应章节；可写view、应用内GPU图、一般资源阶段、异步／远端和版本迁移继续开放。

## 函数内动态拥有位置、补回与循环初始化

普通CPU应用中的固定拥有数组、嵌套记录和变体／Result可以通过合法动态下标直接移出资源叶或拥有聚合。赋值先求出左侧位置和下标，再求右侧；下标有副作用时只消费一次。覆盖释放该位置在右侧完成后仍拥有的内容；移出本身不释放已经交给新所有者的资源。例如，假定State拥有rows及普通epoch，make构造一个完整State：

```b
u32 example(u32 index) on cpu {
    array<State,2> values=array<State,2>(make(3,7),make(3,11));
    if(index<values.length){
        State selected=values[index];
        values[index]=make(3,17);
        println(selected.epoch);
    }
    return 0;
}
```

移出后再读取同一空位或重复移动仍拒绝。补回的是实际下标对应的位置；其他可能元素不因此变空，也不因此初始化。普通字段与dataset内容仍分别满足完整初始化规则，特别是没有合法零值的变体内容，不能借另一个候选的状态获得读取权限。

分支、for／while、break／continue与返回保持各自实际拥有和初始化事实。可证明已写入的范围可以继续读取；未写入元素可以保留为空，退出只释放实际仍存在的资源。write／inout调用的实际输出位置也参与构造分析。下面使用的show是普通read State函数：

```b
array<State,3> values;
if(limit>0 && limit<=2){
    for(i in 0..limit){values[i]=make(3,23);}
    show(values[0]);
    if(limit>1){show(values[1]);}
}
```

拥有值返回及write／inout的每条正常／显式Err返回仍要求声明范围完整。只读、独占重叠和实际view寿命规则保持，动态选择不复制dataset或生成运行时拥有位图。GPU拥有内容可以在CPU应用中移动；实际载荷访问仍要求相应位置，upload／download显式传输，最后真实消费者完成后才释放资源。

本节记录函数内关系。跨普通函数与无源码对象／归档的条件输出位置按下节实现；原机器参数保持。局部控制与初始化状态不进入公开接口，条件输入及循环提前退出的跨函数输出关系按后节交付；其他未完成范围见Issues。

## 跨函数模块的条件资源输出位置

普通CPU函数的返回值和write／inout输出保留资源位置与原下标的关系。例如下列接口移出动态选中的State，再给原位置补入CPU资源；它可由源码调用，也可编译成独立对象或归档交给调用者。

```b
result<State,u32> exchange(inout array<State,2> states,u32 i) on cpu {
    if(i<2) {
        State old=states[i];
        states[i]=make(3,107);
        return Ok(old);
    }
    return Err(149);
}
```

若入口states[0].rows在CPU、states[1].rows在GPU，调用者可按相同i消费实际输出：

```b
let old=exchange(states,i);
match(old) {
    Ok(value) {
        if(i==1) { download(value.rows); }
        emit(value);
    }
    Err(error) { println(error); }
}
if(i!=1) { download(states[1].rows); }
emit(states[0]);
emit(states[1]);
```

此处make和emit采用前述State布局与普通CPU资源访问；download只作用于真实GPU位置。i为0时，原states[1]仍需下载；i为1时，取出的GPU资源下载，而states[1]已被补为CPU资源；i超出数组范围时返回Err，原两项保持。返回分支和所有write／inout输出须在每个实际返回点完整。调用者不传递隐藏的位置标志，编译器不插入隐式传输。

普通转交、递归和独立对象／归档保留同一条件输出合同。view的24字节／对齐8和原机器参数保持。条件输入要求及循环提前退出的输出位置按后节交付；完整资源阶段及其他开放能力继续单独记录，不将输出位置实现冒称全部模块合同完成。

## 条件输入资源位置合同

CPU函数只对实际生效路径中读取或传输的资源提出位置要求。以下函数按i读取一个State；另一项没有本次CPU访问消费者，不会因此被要求也在CPU。

```b
void read_selected(read array<State,2> states,u32 i) on cpu {
    if(i==0) {
        for(row in states[0].rows){println(row.x);}
    } else {
        for(row in states[1].rows){println(row.x);}
    }
}
```

若states[0].rows在CPU、states[1].rows在GPU，调用者可以在`if(i==0)`中调用read_selected；若真正选择GPU元素读取，仍报B266，编译器不会隐式download。动态下标在已证明的数组范围内按同一实际选择处理，普通转交和递归保留原实参条件；字面量实参按实际值代入。

一个输入也可以按声明bool分别要求CPU或GPU。例如`if(!ready){download(value.rows);}`随后读取rows：ready为true时要求入口CPU，为false时要求入口GPU。调用者须按同一ready准备真实存储；仅知道某次运行的结果，不能代替程序里可追查的关系。循环内配对传输按实际回边归还入口位置，不把条件要求提升为无条件假设。

这些条件仅是编译合同，引用声明输入快照，不传入隐藏位置标志或依赖未来输出。源码、无库.h／.b对象和归档采用同一模块合同，引用／view布局和实际资源所有者保持。

循环提前退出的跨函数输出关系按下节交付，与条件输入分别记录；完整资源阶段与其他开放能力继续见Issues。

## 循环提前退出的跨函数资源关系

CPU函数在for／while中显式download后break，调用者可以按同一真实输入条件下载仍驻留GPU的分支。循环可以返回或write一个bool报告实际完成的传输；调用者按该值处理其余分支。零轮保留原存储；continue、嵌套与不同break出口各自交接实际资源，不以最后一次循环条件替代真实入口。

例如局部pick由active开始，在未传输的继续边改为true，done在真实download后置true并break，循环后return done。done为true的路径交付CPU位置，为false的路径保留输入位置；调用者可在if(!done)中显式download。这个关系沿普通转交、自递归／互递归、无库源码对象和归档交接，返回Result标签、动态下标及输入资源来源仍按原公开值关联。

bool报告本身不构成传输：函数只返回active而没有实际download时，GPU内容仍不能被CPU读取。错误方向、缺少传输及真实未知来源继续拒绝。编译器只在原静态条件图中去重相同来源的成立路径，保持无正常来源与未知位置的区别；不加入运行时位置标志、隐式迁移、资源副本或原生求解循环。

原机器ABI保持，资源仍归Frame／blcompute及真实消费者。该关系不等于一般回边不变式、完整资源阶段或稳定热更新。

## CPU自包含拥有聚合与view生命周期

普通CPU原生调用形成含地址或view的拥有结果时，尚未绑定到caller环境的原Returned已经持有其真实资源。原可用性查询同时借用该结果的实际Data引用和环境中当前资源，按同一物理来源、路径及Ownership判断；旧输入绑定Moved不等于已交给结果的物理存储结束。结果只交接已有拥有树，不复制／注册资源或提前发布可用状态，实际释放、独占借用及外部地址的真实寿命保持。临时拥有记录的地址或view字段可在同一完整表达式中立即读取、转交普通值参数或用于实际独占元素修改；其真实拥有资源保持到该次完整消费结束。语句、if／while条件、数值for边界、seek位置、返回及选择分支内已经完成的标量消费分别在原控制边界结束临时拥有关系。普通无借用元数据读取和真正拥有字段移动仍按原字段规则立即结束残余拥有者；静态清理F的既定次序保持。临时记录的地址／view普通字段可作为read address、read view及实际泛型read T参数；原拥有者保持到包含该借用的完整消费结束。被调函数返回同一借用可继续在该完整表达式内消费；独立拥有者及其释放责任不转给字段的read参数。无借用元数据仍即时结束残余拥有者，静态F次序保持。临时借用不能通过后续语句或函数返回逃出其资源寿命；选择式可以从所选临时拥有者取得地址或view，继续在同一完整表达式中读取或交给read参数；只执行选中分支的创建与清理。单侧临时与具名来源、不同拥有记录类型、嵌套选择及多个实参都保留实际分支及资源来源，条件的原值快照不因后续变量改写而改变所选拥有者。无关且已完成的临时仍在原分支结束，临时借用跨语句／函数寿命逃逸仍拒绝。临时dataset取出的普通元素若包含仍借用父存储的地址或view，可在同一完整表达式内立即消费；父拥有者保持至真实消费结束后释放。将该元素记录或地址留到后续语句、或者返回为越出父寿命的借用仍拒绝；不借用父存储的普通值在元素读取后即可结束父拥有者。临时dataset返回值的元素可以在同一完整表达式内直接读取或用address_of取得实际位置，固定／动态长度、泛型与选择式采用相同来源和索引规则。基表达式与下标各求值一次；真实父拥有者保持到该次完整消费结束，地址不能经后续语句或函数返回逃出父寿命。不再把拥有dataset本身当作普通可复制值。函数返回的临时拥有数组可用address_of取得其元素或普通字段的实际地址；数组的真实资源由同一原拥有者保持到完整消费结束，之后按原逆序运行静态F并释放。取址不移动元素，所取地址及内部资源引用不能超出其真实来源寿命；返回局部数组存储的地址仍拒绝。临时数组的元素／普通字段作为write／inout实参时借用原实际位置，不先按值移出元素，父拥有者保持到该次完整位置消费结束。按值取出拥有元素仍移动所选树并结束残余拥有者；read借用及完整临时拥有值分别沿原借用、真实调用和拥有交接结束，不因本项改变它们的F次序。临时及具名Frame dataset的拥有元素可按实际位置交给write／inout参数，实际字段和数组投影使用同一元素模型，调用结果写回同一拥有树；整份替换结束实际旧资源，子资源及后续读取保持真实输出。只读来源、活动外部借用、重叠独占和越出父寿命的地址仍拒绝。Frame元素及明确字段作为inout拥有参数时，调用后读取新地址字段沿同次实际输出；交给参数的拥有树内部旧来源不冒充调用者外部借用。未移交的静态F即使没有后续显式字段读取，仍保留其真实资源来源；同一元素其他字段的F、跨元素F和外部已保存地址继续阻止过早替换。静态F及最后真实消费者的原结束顺序保持。临时拥有数组、嵌套拥有数组或Frame返回值内的dataset字段，以原已求值Place恢复同一资源引用，再取得其中元素及实际write／inout位置；不把dataset元数据当普通可复制值，也不先移出该子资源。普通拥有槽复用原候选树，Frame元素复用原application_owned_argument与实际元素模型，保留父位置、字段布局、初始化、Access及最后消费者寿命。基表达式及各层下标按源码顺序各求值一次；临时父拥有者沿原完整消费结束，取出的地址不能跨出真实父寿命。原只读／原子、越界、重叠和外部借用关系保持。完整dataset视图独占交接时，实际完整作用域及字段投影决定已交付的元素分量；唯一元素的拥有位置完整交接使用同一覆盖关系。调用方后续读取与静态F不再重复消费这些已交付的旧分量，被调方输入／输出及原拥有树继续承担真实消费。未交付字段、跨元素来源及已保存外部地址／F保持；不能把部分元素交接当作整个共同模型已交付。多元素借用沿实际选择来源区分。独立F保存读取借用时，该来源在F实际完成前不可改写；资源所有者的F直接接管拥有载荷，按真实资源拥有及释放关系消费，不把它与另一个资源的读取借用混为一谈；完整子dataset字段可直接交给普通CPU的泛型inout T或write T；尚未替换的语法T不决定资源种类。原入口取得实际Place后，dataset直接交给同一application_place_reference与BorrowedData，其他普通值与拥有聚合分别沿原Reference／OwnedReference交接。原访问、初始化、形状、别名、地址来源及父寿命继续由实际资源和被调合同决定；不把子dataset复制为普通值或再创建拥有者。拥有记录、嵌套记录、固定拥有数组及条件选择沿同一普通调用规则交接；结果资源之外的地址来源继续由当前真实所有者承担，不能借一份无关资源使已结束来源变得有效。

普通CPU拥有聚合可以把dataset和指向其中实际存储的不可变view一起交付。view仍是借用，物理存储归原Frame；移动或返回聚合不复制dataset，也不因元数据位置变化使view失效。例如：

```b
struct Row { u32 x; };
struct Graph { dataset<Row> rows; view<Row> refs; };

Graph make(u32 seed) on cpu {
    Graph value;
    value.rows=dataset<Row>(3);
    for(row in value.rows) { row.x=ordinal(row)+seed; }
    value.refs=view<Row>(value.rows);
    return value;
}

Graph choose(u32 count) on cpu {
    let values=array<Graph,2>(make(7),make(17));
    u32 cursor=0;
    while(cursor<count) {
        let index=cursor%2;
        Graph item=values[index];
        values[index]=item;
        cursor+=1;
    }
    return values[0];
}
```

动态取出只移动选中元素，原位补回也只更新同一位置的资源和view来源。返回values[0]后，其资源和refs共同存活，未选中的仍拥有资源沿实际出口释放。记录、固定数组、enum／Result、write／inout、普通转交和递归共用这份关系；来源及其选择条件随独立对象与归档交接，调用者不需要读取库源码。

只移动本地拥有元数据不移动实际存储，因此仍存活的外部view可以继续读取原资源；把拥有值交给另一函数时则须同时满足该交接的借用合同。实际写入、CPU／GPU存储传输或释放不能越过仍有后续使用的view或元素位置。返回单独view而释放其本地资源、从临时拥有值提取会悬空的view、把一个数组元素的view附到另一个将独立返回的拥有值上，仍然拒绝。合法性取决于真实来源与消费者，不能仅凭两个值的来源集合相同推定关系成立。

来源与访问关系进入原模块合同，view的24字节／对齐8布局及Frame保持。本节支持CPU自包含拥有聚合；拥有元素子资源的完整长度关系／批量增长、持久dataset图的B构造、应用内GPU图、可写view、完整资源阶段、异步／远端和版本迁移仍待实现。普通C ABI不据此开放拥有资源或view值。

## 不透明原生句柄 handle
`handle`是独立的不透明普通值类型，用于原生外部API的句柄／对象身份交接。当前Windows x64普通函数、外部C ABI、独立B模块、记录／固定数组、CPU／GPU身份复制与生成的C++／Rust绑定已有Release和原生结果。handle本身不开放解引用；address的显式外部位置按原生地址章节使用。系统回调和BLEngine窗口系统迁移仍未完成，u64算术按数值章节的实际范围使用。

- 大小为8字节，对齐为8，保持全部位身份。复制、赋值、参数／返回、记录和固定数组沿原普通值规则；跨模块保留精确类型身份及声明布局，名称仍区分大小写，`handle`不属于numeric数值族。
- `handle()`构造全零空身份；`handle(value)`只接受同类型身份复制。同类型`==`／`!=`比较全部身份并返回一个bool。普通构造与转换不接受数值，没有算术、位运算、顺序比较、分量字段、按位转换或隐式真假值；`uint2`不能替代这个源类型。
- `unsafe_handle(usize_value)`显式将完整64位外部位模式解释为handle；参数必须是usize／u64，表达式可以是编译期常量或动态值。它不读取存储、不经过address、不取得或复制对象及权限，也不产生关闭责任；调用者负责原生API身份、哨兵及实际用途。该形成入口不提供取回位模式或handle数值运算。内部沿原handle逻辑分量及既有转换／移位降低，类型布局和ABI保持。例如`const handle INVALID_HANDLE_VALUE=unsafe_handle(18446744073709551615u64);`表达Windows整宽失败哨兵，不能用普通u32值代替高32位。
- 当前Windows x64 C ABI按实际8字节值及声明对齐交接；`read`／`write`／`inout handle`交接真实句柄存储地址。顶层及记录／数组中的句柄叶在生成C／Rust绑定中采用原生不透明指针，内部寄存器分量不是B可访问字段。已有带载荷变体继续沿原显式判别字／载荷存储ABI，不在本项另建对象模型。
- 句柄只是外部对象身份。复制身份不会复制、取得或自动释放对象，也不证明身份非空、有效、未关闭或属于正确API。实际创建、使用、异步消费者和关闭责任由外部API合同及原调用者履行；不会增加句柄注册表、隐藏Close、全局引用计数或Frame资源副本。用户资源拥有与确定销毁仍有独立缺口。
- 对身份的计算、存储或传输不迁移外部对象、不扩大权限，也不把本地对象变成GPU或远端可解引用能力。原生C声明继续是信任边界，接口的资源有效期和权限仍须如实声明与履行。
- write handle须遵守完整或明确声明的条件输出初始化合同。可能失败且不写输出的API必须准确声明条件，或按真实签名使用已经初始化的inout位置，不能假称正常C返回就成功写出句柄。

```b
extern "C" handle current_process() on cpu
    from "kernel32.dll" as "GetCurrentProcess";
extern "C" u32 process_id(handle process) on cpu
    from "kernel32.dll" as "GetProcessId";
```

handle使用原POD布局、普通调用和实际标量位值，不增加运行时句柄登记或Frame所有者。生成接口按当前类型能力和真实C签名交付，源类型支持与二进制兼容分别处理。

## 条件输出初始化（C-020.5.1）

普通on cpu／on compute／on gpu函数沿现有ensures声明实际输出的初始化条件；compute／GPU实例函数的条件与实际当前位置在原源码展开中交接。前文要求所有返回完整输出的规则适用于未声明条件的分量；本节明确条件声明的范围：

```
bool read_point(bool found, write Point output) on cpu
    ensures initialized(output, returned);
result<void,u32> read_point(bool found, write Point output) on cpu
    ensures initialized(output, returned.Ok);
```

initialized的第一个参数是write／inout参数或其声明字段；第二个参数是bool条件。returned表示这次实际返回值，returned.Ok／returned.Err及已声明enum分支名表示活动分支条件；普通bool返回可直接使用returned或!returned，原生整数状态可以比较returned != 0。条件可使用按值输入快照、字段、资源长度、常量及原有纯布尔／数值运算，不执行函数调用、存储访问、传输或隐藏副作用；它不是调用后再次读取已修改输入。returned仅在该条件上下文表示返回值，名称身份仍精确区分大小写。

合同允许只在某些返回分支完成指定位置，例如output.x只在returned时写出，output.y只在!returned时写出。未涉及的分量继续要求每条返回都完整；覆盖同一分量的多个声明按条件并集交接。读取和read／inout借用仍须在当前路径满足真实初始化关系。调用会撤回声明范围的原初始化知识，再按本次真实输入与返回条件授予可读权限；不能凭调用前的旧值或物理零值推定失败分支仍可读取。资源在调用前经过分支合流，也适用同一规则：完整初始化分支不是永久可读许可，未被本次输出涉及的字段与原分支关系保持。函数返回值本身仍必须完整。

拥有资源还要求初始化条件对应实际交付：被声明分支须归还相应活动资源，其他分支不能留下未声明却实际仍拥有的内容；不满足时报B229。接口不把错误返回解释成自动销毁输出或回滚，也不复制所有者。调用者按同一返回条件拥有、移动或释放这些内容；实际最后CPU／GPU消费者及外部view继续沿原Frame和借用关系。局部未知条件不变成跨模块的拥有许可。

普通C ABI可在extern C声明上给出ensures initialized(output, returned != 0)等真实外部合同；它是调用者对外部接口的信任声明，不验证DLL的实现或扩张普通C ABI的资源范围。B定义则逐真实返回路径检查其承诺。声明、定义、递归、无源码对象／归档及链接合同需要保持同一条件，不能在模块编码中丢成无条件完整。条件输出沿原类型化模块合同交接。初始化条件本身不增加运行时标志或机器参数。compute／GPU普通实例函数可按真实输入或返回条件写出获准的元素／字段；返回点在该条件成立的路径上复用原初始化位置查询，调用点按同一条件取得输出。它不授予跨元素非原子散写，也不代表完整设备普通调用／递归、异步失败、用户资源销毁或完整C-020.5已经交付。

条件中的模块常量在原函数合同中保存实际参与投影的类型和值；const参数按具体实例代入，不能只保存名称而忽略含义变化。条件改变或实际参与的常量改值时，链接拒绝不匹配的旧调用对象；仅私有实现改变且公共合同保持时可重链。初始化来源仍由原Builder编号器统一分配，形参和后续dataset／内联来源不共用身份。

## CPU用户拥有值与静态释放（C-020.3.1）

`owned<T,F>`是一份具有静态释放函数的T载荷。普通CPU函数以`own(value,F)`构造，也可写`owned<T,F>(value)`；F必须接收一个按值T并返回void，使用普通`on cpu`函数或具有准确声明的外部函数。泛型和const按T推导，声明处写函数名称，不把F当作运行时函数值或闭包。T与F共同构成类型身份；包装的大小、对齐、字段载荷与T相同，F不占机器存储。

例如，已准确声明CloseHandle与GetLastError的资源库可定义：

```b
void close_process(handle value) on cpu {
    if (!CloseHandle(value)) { println(GetLastError()); }
}

owned<handle,close_process> take_process(handle value) on cpu {
    return own(value,close_process);
}
```

资源库应在.h声明释放函数与公开拥有类型，.b提供普通实现。调用者按实际API合同交付可关闭的资源；语言保证释放函数的静态调用与包装的移动关系，不把函数返回或原始身份复制当作外部关闭成功。需要可失败的显式结束操作时，以普通函数／Result表达实际结果和剩余所有权；没有隐式吞错、重试或自动回滚。

包装按值移动，不能隐式复制；记录、固定数组、enum／Result及普通泛型可组合这些值。`read`借用不取得销毁权，`write`／`inout`沿原实际位置交接。`.value`提供原载荷的普通字段与位置借用；若载荷自身拥有资源，按值拆出须用`into_inner(owner)`整份移交。该操作消费包装并取消其F调用，原dataset／拥有聚合及view关系随T继续交接。例如，下面的raw由接收者继续负责，其产生不等于外部对象已释放：

```b
let owned_process=take_process(actual_process);
let raw=into_inner(owned_process);
```

普通CPU的作用域结束、return／try、break／continue，以及原B资源调用失败的实际函数退出，都按该路径的真实拥有事实调用F；read借用不取得释放权，正常write／inout交回与失败时实际剩余树结束分别处理。用户拥有绑定按声明逆序，记录／数组按字段／元素逆序处理；覆盖先求值右侧，再调用旧包装的F，最后交付新值。F接收并拥有原T，F函数中T自身的资源继续沿原规则清理；包装不再重复释放它们。已接管的记录／容器释放组在F失败后继续其余真实F，只保留一个当前拥有错误，必要时通过blink_error_join消费两个错误并保留原文本；组结束后才向外传播，不重启旧树，不因一个F失败遗弃剩余F。空身份不是隐式跳过释放的条件，是否可接受由真实资源API及F合同决定。进程强制终止、系统异常展开及异步取消不在本项交付范围。

F是实际后续消费者。载荷含view时，其真实来源保持到F调用结束，不能因为没有后续显式源码读取就先写入、传输或释放来源；移动或into_inner会结束原包装的这次待执行消费，并把实际借用交给新接收者。条件构造只在实际取得包装的分支持有消费关系。普通记录与固定数组按原逆序结束每个拥有字段前，原拥有树先交出该字段，再将尚未执行的其余静态释放消费者的实际借用交给同一释放检查。已移出的包装和已完成的释放调用不再保留消费关系；不能先结束其来源再调用F，也不改变字段或元素的释放顺序。所有关系复用原Proof、Presence和初始化位置，资源仍归原Frame／blcompute，不新增运行时拥有位图、资源登记、引用计数或清理线程。

条件输出同样使用`ensures initialized(output,returned)`等已有合同：被交付分支须真正构造拥有包装，未交付分支不能留下未声明的拥有内容。初始化条件与拥有条件随源码调用、独立对象和COFF归档交接；原始未初始化或已移动路径仍不能读取载荷。私有实现与公共合同分别处理，不从链接成功推定外部资源已经关闭。

静态释放身份随类型合同交接；不增加旧格式兼容路径。已有机器载荷、view布局和C资源入口签名保持。拥有包装不进入普通C ABI，不成为GPU元素拥有模型、C函数地址签名值、系统回调或完整热更新能力。源语言实现、真实CPU／GPU消费者、提交发布与BLEngine实际替换分别记录。

## 普通 CPU 任务

`start_task(worker,input, positions...)` 启动静态绑定的普通 `on cpu` 函数。拥有记录 `input` 按声明顺序保存工作函数的按值参数，后续实参依次提供 `read`、`write` 和 `inout` 位置。启动沿普通调用核对实参、初始化、存储位置和函数前置条件。需要在两个 Frame 之间交接的拥有 dataset 及其活动子资源使用 CPU 存储。

共享 `read` 允许实际位置上的其他读消费者。`write` 和 `inout` 在工作完成前保留实际位置的独占访问。整份数据、投影字段和普通位置沿同一内存合同交接，相邻独立字节范围可分别访问。借用位置由原调用方持有外层存储，工作 Frame 在借用期持有需要转移的子资源。`write` 从实际工作受理后结束旧拥有元素，按输出合同交付已初始化的新元素。`inout` 交接实际活动值，旧值替换和失败清理使用原类型及实际位置。失败出口仍留在借用位置上的子资源遵守对应入口的 CPU／GPU 驻留，调用方沿已有驻留事实和原错误中记录的已消费位置清理。

启动返回 `result<task<T>,task_start_error<I>>`，T 来自工作结果，I 来自拥有输入记录。Windows 工作句柄创建失败时，Err 带实际系统错误码并归还拥有输入，原借用位置保持其受理前的状态。工作提交在句柄建立和实际输入交接后发生。

`wait_task(work)` 消费唯一拥有任务，等待工作完成并交回结果及实际写入的位置。工作函数无业务返回值时使用 `task<void>`。任务可按值移交，未显式等待的任务在实际拥有范围结束时等待并清理结果。结果中的 dataset 保留实际类型和嵌套拥有关系。条件输出沿工作返回合同确定哪些位置已初始化。Frame 转移失败时，仍由工作持有的 CPU 活动值在原 Frame 中按类型析构，已消费位置沿原错误交接，原工作错误保留。

应用和独立模块共用原普通调用及对象合同。Windows x64 使用原生 CPU 工作池。取消及异步失败的其余能力见 C-020.4.1.3。

## CPU拥有元素的dataset

拥有dataset元素的按值读取直接转移所选唯一拥有树，基对象与下标各求值一次；局部空位由编译期拥有状态表示，真实补回恢复完整性，完整参数／返回仍要求全部活动元素完整。临时父集合只在其最后真实消费者结束后释放。for／while的回边、continue、break、零次与自然出口交接各自剩余元素，最后静态F和CPU／GPU子资源沿原释放链结束；局部空位不进入普通集合ABI。已移出且尚未补回的元素不能再次读取／移出，也不能把仍有空位的集合交给普通完整dataset参数或返回；实际仍存活的子view和外部借用继续约束移动。该能力使用现有元素布局与资源ABI；sequence.insert的库实现及正式种子采用分别沿原台账交付。

普通CPU应用可用`dataset<T>(length,F)`构造拥有元素或含view元素的动态序列。T可以是含dataset字段的非递归记录、已有固定拥有数组／活动变体，或`owned<U,Dispose>`；静态F也可返回引用同一元素拥有子资源的view聚合，记录字段可继续包含这种元素的dataset。每个view须保持真实来源，不能借出F将释放的局部资源。F是明确的静态普通`on cpu`函数引用，接收一个按值u32下标并返回完整初始化的一个T，可沿已有泛型／const规则专门化，不是运行时函数值或带捕获闭包。构造长度与固定形状仍按原u32／N合同检查。

```b
struct Item { dataset<u32,1> values; };
void dispose_item(Item item) on cpu {
    if(!println(item.values[0])) { return; }
}
owned<Item,dispose_item> make_item(u32 index) on cpu {
    let values=dataset<u32>(1);
    values[0]=index+1;
    return own(Item(values),dispose_item);
}
void use_items() on cpu {
    let items=dataset<owned<Item,dispose_item>>(3,make_item);
    items[1]=make_item(7);
    let replacement=dataset<u32>(1);
    replacement[0]=19;
    items[1].value.values=replacement;
}
```

F按实际下标从零递增，每个位置只接收一次真实移动结果。长度已证明为零时可直接用`dataset<T>(0)`构造空拥有序列，不需要生成器，也不执行元素构造或释放；显式提供F时仍检查其真实签名，但不调用。正长度或未知长度的拥有元素或含view元素仍须由真实F逐项产生，不能用零地址造view、零构造拥有值或复制填充；固定形状与类型规则保持，空序列不能读取元素。已证明唯一新增一项的resize沿下节移动一次填充值。例中退出时按元素逆序调用各自静态释放函数，随后释放外层存储。正常块结束、return／try、覆盖及活动变体／Result使用同一拥有与清理关系；整份序列的局部移动、按值形参、返回、记录字段及独立对象／归档交接唯一拥有者，不复制子资源句柄或另建运行时拥有表。

普通函数以原参数资源路径交接共同元素中子dataset的真实CPU／GPU位置；字段与共同元素是不同路径步骤，共同元素不表示下标零。输入要求、实参、返回及write／inout输出沿同一原位置合同与Data.elements绑定，支持记录、固定数组及活动enum／Result的嵌套；零长及非活动载荷不虚构资源，未知位置不猜成CPU。原静态释放F的内容访问与显式传输消费实际位置，CPU不能读取GPU载荷。独立对象／归档交接同一合同；本关系不新增机器参数、资源表或运行时扫描。构造在CPU写入元素载荷及子资源元数据；子资源的CPU／GPU存储位置独立保持，仅保存句柄与长度不要求读取其载荷，也不隐式下载。实际元素元数据读取、静态F中的内容访问及upload／download继续提出原位置合同；GPU内容要由CPU读取时显式download，最后真实消费者结束后再释放。将整份拥有元素序列元数据上传后，实际CPU元素释放读取前同样须显式download。

编译期由原Data保存所有已初始化元素共同成立的值、资源、初始化与地址来源事实。共同前提不表示不同元素的值相等；实际装载将所选分量的共同输入数值身份实例化为本次读取的寄存器，保留共同范围／非零条件及原资源／view借用来源。不同下标的读取、比较及分支各自消费真实值，不把共同合同当作同一份载荷。生成、元素写入、移动／返回及函数／模块参数结果沿同一模型交接；部分或未知覆盖保留共同事实的合流，已证明唯一实际元素所选字段全部覆盖可替换其旧事实，一般批写覆盖仍待完善。实际指针字段的原位置读写效果随普通调用交接，写后旧数值事实撤回；未初始化位置不能借元素输入或符号来源取得读取权限。

显式后续元素读取与静态F都是实际借用消费者。提前改写、resize、迁移或释放其来源仍拒绝；调用F接管后，不再把同一F当作另一个未来拥有者。返回的元素地址必须来自随返回拥有树交付的资源或仍由调用方保持的真实来源；局部栈／将释放资源的地址不能逃逸。这些合同随无实现源码的对象与归档保持，不改变Frame唯一物理所有权或资源C ABI。

含view元素的整值、字段和普通read调用按原共同元素模型绑定实际子资源来源。view引用元素内的dataset时，其原ElementComponent资源根交接为实际父dataset及字段路径；多层子dataset逐层保留该父关系，未装载的兄弟资源元数据不成为无来源借用。普通read参数与返回view使用同一关系；调用输入保留原调用者数值／地址快照及条件，只将元素借用绑定到真实父资源，不重复代入这些已求值事实。原拥有者保持到最后真实消费者，局部父存储不能通过返回view逃逸。最后使用点之后，可按原拥有字段规则替换相应子资源；其他资源字段的独立借用仍按真实路径处理。

同一dataset的内部view借用按实际元素选择交接：借用rows[0]后可替换已证不同的rows[1]子资源；同元素替换、父释放及未知可能交叠继续按真实消费者判断。普通read函数返回单一来源view或在两个子view之间选择返回时，长度沿原数值条件和共同来源交接。两个实际选支都已证明非空时，调用者可直接使用下标0；已知选支只消费本支事实，可能为空或来源未知时仍需相应长度证明。不同长度不因此相等；含view记录、普通数值长度返回和多层转发沿同一关系交接。普通CPU拥有dataset中的含view记录支持整元素及拥有字段的完整移动赋值；原始view载荷写入及普通write／inout独占借出仍由B185拒绝。

含view拥有元素的赋值沿原Returned求值一次，旧拥有内容在RHS求值结束后由原drop_returned结束，实际新载荷只写入原元素位置。此时活动借用保留未覆盖位置、外部旧view及新值实际持有的旧view；被完整覆盖位置未来的新值读取不借用旧内容。RHS只读取旧值并在返回前结束借用，可以随后替换该元素；新值若仍持有即将结束存储的view，则继续拒绝。新值的原临时绑定保持到最后capture_element_model消费者结束；ElementComponent／ElementParent属于各自元素作用域，物化和调用时按实际父资源及字段路径交接。每个子资源的原Proof保留同一真实资源的生产来源与物理别名；整份拥有树按值移交、覆盖或释放时，原借用检查同时消费这些子资源，不能只检查外层句柄。修改普通数值兄弟字段保留Data.root的实际来源，不能因清理平面载荷数值Proof而丢失资源关系。 普通函数整体交回一个拥有dataset后，其内部view继续引用实际返回拥有者持有的存储，记录与数组元素遵守同一资源规则。嵌套dataset未改写的入口子树按原合同取得本次实参已有的共同元素事实及实际位置；普通函数二次转交、记录中的第二资源根与有限递归拥有树沿同一关系继续，不要求调用者增加与程序无关的元素前提。真实写后的Unknown不重新取得入口事实，已经形成的返回模型不被入口替换。两个完整嵌套拥有输入按分支交回一方时，未采用方沿原拥有链清理；元素清理存活查询只为实际静态F可能读取的存储保留借用，不因普通类型含view且入口尚未展开就制造未知读取。普通内容读取及真正F的未知来源仍须满足原合同。复杂组合的条件合流查询成本仍见Issues C-027.30.6.2。普通函数实际深读所需的有限共同元素路径及数值输入随原函数合同交接，调用者无需添加与程序无关的[*]前提；源码缺席的obj／lib消费相同描述，空view与真实写后的未知长度仍须满足原索引要求。已闭合合同在调用点保留实际输入来源，资源可用性沿当前路径中的拥有树判断。原输入绑定已移动不能单独作为存储失效的依据；实际释放、覆盖及活动借用继续按原来源检查。

仅移动拥有元数据并保持实际资源时，旧的普通view／地址元数据可继续引用仍存活的资源；资源按值交给函数、覆盖、迁移或释放不能越过调用者仍活跃的借用。完整覆盖的长度及祖先条件直接进入原Proof与拥有模型：实际仅一元素且所选位置完整覆盖时，旧view来源被新值替换；其他情况保留可能未改元素的共同来源。条件在实际调用输入处代入，不从未知长度推定完整覆盖。普通函数返回及无提供者.h／.b的对象／归档交接外层和元素子资源的原输出路径；空集合和非活动载荷不生产子资源。编译期来源不形成另一份运行时拥有表、资源副本或释放链；不同元素的精细借用仍见.4：读取未覆盖元素的消费者与被替换元素的资源身份仍须区分，不能从被覆盖位置的最后消费者规则推定该剩余能力已闭合。

普通CPU函数从dataset合法元素位置读取整数并返回时，包括view长度，输出保留本次独立数值快照，并沿原Proof合同交接该元素分量的共同整数范围与非零性质。调用者按实际dataset当前共同模型收窄该输出，多层普通转发、嵌套dataset及read拥有元素参数复用相同规则；源码缺席的对象／归档保持关系。不同元素可具有不同数值或长度，共同范围不产生相等身份，不把某个元素值或长度复制给其他元素。已捕获整数保持原快照；实际写入按原共同模型撤回失效事实，新读取不能使用写前范围。未知或可能为零／越界的输入仍须满足原运算与下标规则。本关系当前交接整数区间及非零，不代表任意跨元素配对、一般关系求解或浮点次序推导。

普通CPU的`read dataset<T>`允许T拥有资源或静态释放身份，沿同一真实借用读取length及合法下标处的普通标量字段；原外长度／下标、CPU内容位置、同根只读别名和实际字段步幅继续适用。只读入口不交接拥有权，callee不释放父存储或元素静态F；最后由原caller拥有者按逆序结束元素，再释放外层。read不能写入、替换、移动或作为拥有结果返回。完整dataset自身作为read T实参时，实际名称、拥有字段及嵌套位置同样只读借用，推导与显式T保留元素类型和固定形状。重复只读别名、多层转借与返回view沿原借用及最后消费者规则使用，实参及其中下标按原顺序各求值一次；不因读取而移动或复制caller资源。合法元素位置还可作为`read T`实参，记录字段、非连续列投影和固定拥有数组的静态／动态选择沿原Data.elements及实际Place取得同一拥有模型；按形参既有Value调用约定交接真实元数据，不把引用地址当作拥有载荷，也不复制或移动资源。被调只读函数可沿原长度条件读取CPU子dataset载荷，或返回其view／地址；GPU子资源元数据仍可读取，CPU内容访问和view构造仍要求原真实CPU存储，不能隐式下载。

子资源来源同时保留实际载荷与父拥有关系。对真实载荷的写入继续与活动view冲突；父dataset的移动、覆盖、传输和释放还沿原拥有树检查其真实最后消费者。活动子view不冻结无关的父标量字段，条件选择只约束实际可能来源；原分支、循环、记录／数组、转发和无实现源码对象／归档交接同一关系。返回view不取得拥有权，也不能将read位置升级为可写地址；本函数将释放的局部父资源不能借出。父拥有资源与引用其子资源的view可由同一拥有聚合一并返回或消费，最后使用后原拥有者可继续交接。实现仅在原编译期来源图保留子资源槽及父关系，不增加运行时拥有表或第二释放链。

命名动态元素支持直接`rows[i].values.length`、满足原范围条件的子dataset索引、`view<u32>(rows[i].values)`和`address_of(rows[i].values[j])`。记录字段、非连续列投影及固定拥有数组选择沿同一实际位置读取，下标按源码顺序求值一次。该入口只读取原Data.elements共同模型及实际元数据，不复制、移动或重新拥有元素；临时拥有数组及Frame返回值中的dataset字段同样沿实际位置取得子元素或其地址，父拥有者与初始化、访问、真实来源及最后消费者保持；临时数组直接元素／普通字段仍沿前述数组位置规则。完整字段的泛型独占传参按实际dataset位置交接；独立F保存的读取借用按最后消费者保持，不能借索引或字段投影升级写权限。

直接view与地址保留实际子资源和父拥有依赖。父仍被实际借用时，不能在同一调用的另一实参中按值消费、覆盖或传输父；普通view／address、read位置及含借用记录按各自真实类型交接相同来源。两个只读实参可指向同一子资源；独立父拥有值及无关head字段修改继续允许，最后消费者结束后可交接原父。对象／归档在缺少实现源码时交付同一合同。

声明的固定长度、共同nonempty前提及已经捕获的view沿各自原数值合同消费。普通CPU同一实际执行段中，`if(rows[i].values.length>0)`后的`rows[i].values[0]`可消费同一真实子资源的长度事实；同样，`if(data[i]!=0)`后重复读取`data[i]`可用于整数除数。常量及同一实际动态下标、普通条件和选择式、循环体内重新建立的条件均沿实际装载判断，不要求改写成捕获变量或访问函数。

数值关系只在实际Scalar、地址、下标、stride、字节偏移相同，且中间没有可能覆盖该字节范围的写入时成立。可区分的其他元素／字段写入、独立分配及只读调用保留关系；同位写入、变化下标、可重叠写和未知地址写不能消费旧内容事实。普通Reference的write／inout权限仍允许真实修改，函数体当前只读不自动形成read保证；原调用合同负责交接效果。已捕获普通值及bool继续表示原数值快照，不变成当前存储的别名。拥有、初始化、借用与最后消费者规则另按原合同成立；共同元素模型的逻辑槽不是调用者SSA寄存器；分支、短路、返回及部分元素写回沿原拥有树合并模型既有Proof，不能再次捕获模型槽。部分写回只投影实际写入的分量；已知写入下标区间外保留原关系，区间内和未知选择保留旧新共同保证，不把可能未写当成全部未写。实际运行值仍按原SSA身份合流；application_element_proof直接投影模型已有Proof，在同一bindings代入本次真实资源与已求值下标，普通数值、条件、地址／view及子资源长度消费同一实际选择，不把缺失来源补成同号Register。共同元素模型不成为不同实际元素的相等数值身份；每次读取使用自己的真实装载快照，DatasetContent的共同范围／非零继续由CommonRange交接。整数比较和普通返回保留该范围与实际分支的关系；调用输入代入后，原条件查询排除零分支才交接非零结果，不要求操作数事先已经持有独立条件。Bool条件沿同一关系继承已确定的共同true／false，原事实集合中的实际Bool命题区分它与整数非零。静态factory的序数沿原完整Proof绑定为对应元素层次的ElementIndex，读取时由本次实际下标实例化，不把不同元素的值视为相等。共同元素模型不表示不同元素数值相等。

每轮重新执行的while条件可向本轮正文提供实际读取事实，例如`while(data[i]!=0)`中的`100/data[i]`，以及`while(rows[i].values.length>0)`中的`rows[i].values[0]`。短路与选择条件只沿当前已证明实际执行的分支交接读取来源；未执行分支不提供事实。条件值来自可变bool时使用该次条件读取之前的真实写入值，后续bool变更不改写旧条件快照。变化下标、同位写、允许修改的引用或未知地址写仍不能使用旧内容事实。静态不可达分支的整数非零检查保持原规则，不能从矛盾路径取得除数许可。

循环外已证明的非零或子长度条件，在实际地址与下标稳定、循环可能写入不覆盖该读取时，可用于while／for正文的重复读取。独立直接分配的无拥有资源元素／字段写，以及只分配新存储的内部调用，可以保留另一分配的既有读取事实。同一存储中，原写分析能确定下标及实际布局的元素、记录字段和固定数组分量，也按真实Scalar字节范围保留不受覆盖的读取；下标可能被改写或布局未知时不能据旧常量推定不相交。可写dataset实参的内容位置也按真实根参与不相交分析，调用本身的实际／未知写仍须独立满足同一不覆盖条件。同位或未知地址写、拥有内容结束、可能改写的调用及下标变化不能据旧读取获得许可。dataset(length,factory)的真实生成调用参与同一写效果：普通静态／泛型纯生成函数可保留旧事实，可能通过外部地址改变已有数据的生成函数必须使旧元素读取与局部槽数值事实失效；每轮重新建立条件后仍按本轮实际值检查。零长度不调用factory，当前扫描识别字面／静态长度及已有不可变U32零值，可变旧零不代表后续构造为空。每轮新条件及已捕获旧数值／bool继续分别按原来源使用。普通静态／泛型CPU值调用在原闭合访问合同确认只读且不返回拥有内容时，可保留既有元素读取和局部数值事实，包括循环正文或嵌套分支中新声明的泛型实参、只读address参数及普通记录结果；实际泛型实例按调用处的词法类型确定。循环正文let新建dataset的元素类型及显式静态N也从原构造类型取得，不要求改写为显式局部类型；动态长度不参与const形状推导，长度和factory仍只由真实构造求值。可能通过按值地址改写局部值的普通B调用，与未知／外部调用一样使原局部常量和非零事实失效；每轮新条件使用本轮实际值，不能把旧常量替换新读取。签名没有inout不代表实现无写。闭合普通CPU调用只写既有write／inout dataset实参内容时，真实写仍按这些实际位置处理，可以保留另一真实分配的读取事实；前置普通值或记录引用参数、泛型及已编译对象／归档沿同一合同交接。未覆盖的Atomic及其他未知写不从此局部范围推为只读。按值交付元素无拥有内容／清理F的dataset时，原唯一拥有、保留消费者及别名检查继续生效；闭合调用只写已交付存储，可以保留另一存储的读取事实，包括循环本轮新建的拥有实参。内含指针的指向写与清理F等其他实际效果不能由此当作只读。闭合合同确认包括Position在内没有实际写入或拥有消费时，write／inout形式权限不再单独撤回dataset或标量／记录引用的旧事实；首次声明／导入完成后也只删除同一次调用的参数权限写。真实Reference写、资源拥有消费以及未知／外部／间接调用仍沿原合同和Position交接。实参表达式副作用及write参数初始化仍独立按原职责检查。生成器建立的共同非零及范围事实也必须经过循环可能写分析；真实可能写会撤回被写分量的旧共同数值，不能许可下一轮对已写零位置的除法。未写字段和独立存储的事实可保留，每轮重新判断使用本轮实际值，已捕获旧值继续表示原快照；可变动态下标经短路前置C调用后的范围缺口另见C-005.14，静态不可达正文继续按C-005.12的现行检查处理。上述当前分析范围不限制合法程序的长期表达能力；普通CPU顺序读取也不把GPU、cooperative或原子并发装载自动等同。

CPU可写dataset的整元素位置支持`rows[i]=fresh`唯一移动替换。原外层下标与实际字段投影按源码顺序求值一次，先取得旧元素及其真实子资源元数据，右值成功后结束旧元素的静态F和拥有子树，再把新载荷写入同一位置；新值不能继续按原拥有者读取。若左侧或右侧try返回Err，没有本次写入，父dataset及已产生的临时拥有资源沿原退出释放。活动子view、只读参数、越界与右值提前消费父dataset仍按原借用／范围合同拒绝。记录、固定拥有数组、活动enum／Result、宽数值及无源码对象／归档使用同一交接；空变体不产生不存在的资源载荷。

编译期只更新原Data.elements共同模型：部分覆盖按原ElementUpdate交接的可能写入下标区间合流已存Proof，区间外保持原元素关系，区间内和未知选择保留旧新共同保证；仅在已知唯一实际元素完整覆盖时由新元素给出共同事实。共同前提可保持，也可因新值不满足而撤回，不把最后写入当作所有元素。静态生成器算术分量到共同nonzero的独立分析缺口见C-005.15，不能据已有常量事实的保持关闭该项。动态元素内沿记录字段和固定数组选位的混合路径支持唯一移动替换，例如`rows[i].value.values=fresh`、`rows[i].values=fresh`及`rows[i].children[k].value.values=fresh`；多级固定数组同样沿实际声明布局选择。外层下标、各层数组下标和右值按源码顺序各求值一次，右值成功后仅结束所选旧字段的静态清理和拥有子树，再接收新字段；父静态F在其原生命周期结束时才执行，兄弟字段、父普通值及共同事实按实际未改内容保持。字段可以本身是拥有记录、完整固定拥有数组或活动enum／Result，不复制拥有资源；只读参数、实际活动view、右值提前消费父资源及已移动新值仍拒绝。try失败沿原退出清理父与已产生临时值，没有本次字段写入。源码调用及提供方h／b缺席的对象／归档使用同一合同。

固定数组的动态下标选择只改实际选中的旧字段；所有可能未改的位置继续贡献原共同前提，不能把某一次写入的新值当成全部元素或全部数组单元的保证。普通数值数组写入同样更新实际分量的共同事实，改写后不能沿用已失效的旧常量或非零前提；未改兄弟字段保持。函数按值返回同一已改写dataset后，原已移动绑定不再为新读取提供调用前旧事实。

同一实际父拥有树内也支持`outer[i].rows[j]=fresh_item`和`outer[i].rows[j].value.values=fresh`，可以包含固定数组字段和更深子dataset。各层下标与右值仍按源码顺序各一次；只沿原父位置的write／inout许可写入，真实read／atomic与活动view继续受原限制。右值期间保持完整父子借用，不能先消费或释放祖先资源。函数返回所持的原调用来源继续关联同一父模型；无提供者源码的对象／归档交接同一事实。

**动态元素按下标区分资源位置。** 同一拥有容器的不同实际元素可以持有不同CPU／GPU子资源。原模型按已经求值的各层选择保存位置成立的条件：替换只改变所选位置，已证不同的元素保留其来源，可能相同或未知时按真实可能位置检查。下标按源码顺序求值一次，后续改写变量不改变已捕获快照；源元素移入其他集合也不按目标下标重新解释源选择。

普通函数与源码缺席对象／归档沿同一参数、字段及选中输入域交接实际元素访问。资源族内部序数与外层选择按原模型层次代入，位置选择不授予普通值相等或借用权限。空集合不生产元素事实；选择范围、初始化、活动借用、唯一拥有及最后释放仍消费原合同。无法证明实际CPU位置的访问继续拒绝，不增加逐元素表、运行时检查或隐式upload／download。

写入沿实际父／子作用域更新原模型。普通值与借用的共同保证只有在每层祖先、当前dataset及实际选择已证明唯一且完整覆盖时才替代全部旧保证；其他情况保留未改元素／字段的共同保证。资源位置分别按实际选择条件更新。这不表示不同元素数值相等，也不按最后一次写入推定所有元素非空或位置一致。条件返回沿原输出合同保留各可能输入共同成立的父／子长度范围；动态集合中不同元素的CPU／GPU位置沿实际选择条件交接，未知选择不获得其他元素的CPU事实。元素按值取出转移所选唯一拥有树，父集合保留其余元素；含view元素的静态F构造按本节交接来源，精细借用按实际元素选择交接；嵌套物化沿原Place.parent把当前及祖先已求值选择交给同一ElementBindings；含view拥有元素完整移动与普通inout入口按本节各自规则使用，普通inout条件写回的子资源按实际分支及本次bool实参交接位置；分支环境合流递归保留原共同元素树，CPU分支可读取实际CPU内容，实际GPU来源继续要求原显式传输。

普通CPU函数支持`inout dataset<T>`独占借用拥有元素序列。参数须已初始化，父存储和静态F继续由调用方持有；被调方可修改普通字段、移动替换整元素或拥有字段，不能重绑、移动、传输、扩缩容或释放这份借用。实际旧子资源在替换时按原静态清理规则结束，父及未改兄弟保持同一拥有树。read入口继续共享读取，不能提升为独占写入。含view元素仍遵守本节对应写入和精细借用的实际能力范围。借用期间整份输出中仍存活的view包括实际内嵌子dataset的元素；只替换其指向的数据而保留该view时须拒绝，完整替换旧子容器则随原生命周期结束其内部view。

调用前检查实际载荷与所选投影中真实子资源的借用，活动子view必须在可能结束其所有者的inout调用前完成最后使用；未涉及投影的兄弟资源仍可保持独立借用。返回时交接实际修改后的共同元素事实与CPU／GPU来源，空函数及未写字段保持输入关系；清空某项或写零后不能沿用原共同非空／非零前提。非完整覆盖保留可能未改元素与新值的共同保证，只合流实际写入的标量分量；完整覆盖的原长度关系可在调用处由实际参数确定。合法CPU子资源可按原范围读取，GPU载荷仍须显式传输。源调用、泛型及提供者源码缺席的对象／归档使用同一合同与原Frame。

部分条件输出中子资源的CPU来源仍可能报B266，属于原Dataset输出选支模型的未完成能力，见C-020.1.6.6.3及C-020.1.6.4.2.2.3.3.2.1；直接替换成功不表示所有条件来源已支持。实际未知或GPU来源继续按真实关系拒绝CPU内容读取。

整份拥有元素序列继续按原规则交接，元素不能按值复制。可证明的单项末尾增长按下节resize语义移动填充值一次；尾部取出按下节take_last交接唯一拥有值；批量增长仍未实现。普通CPU函数的共同元素[nonzero](#common-element-nonzero)与子资源[nonempty](#common-element-nonempty)已按显式[*]选择交接；[index关系](#common-element-index)既可指向同一元素的实际子资源，也可指向一个独立全局dataset／view／固定array。静态F所需事实仍须真实成立，不以类型名称、一次局部写入或源码／链接成功代替证明；递归容器、宿主持久共享view图的B构造及完整资源阶段保持各自未完成范围。

## CPU拥有dataset的显式变长

`resize(data,new_length,fill)`在普通CPU流程中按值消费一份完整初始化的拥有dataset，交回同元素类型的新拥有值。以let绑定返回值时保留完整dataset元素类型，后续普通泛型调用据此推导类型参数。三个实参按普通规则从左到右各求值一次。new_length为当前u32长度，fill须完整初始化并具有同一元素类型。可复制、不含view的普通值可以填充全部新增元素，支持记录、数组、矩阵和活动变体；即使长度未增长，其求值和实际副作用也保留。原值与新长度共同覆盖的前缀保持，复制或移动只读取全部声明分量，类型填充字节不作为字段读取。普通值长度为零有效；固定形状输入可消费，新结果为动态形状，交给固定形状时仍证明实际长度。 普通值与拥有值沿同一Data.elements捕获填充值，并与可能保留的旧前缀合流；不因普通值类型把结果模型换成未知。数值、标签和真实子资源事实仍按原模型及实际消费者交接，拥有值的唯一移动约束保持，新根存储身份不继承旧长度／下标许可。

拥有填充值只交给已证明不回绕的唯一新增位置，即new_length恰好为原长度加一；这不是运行时广播或复制。实际前提可由常量、原数值来源与合法控制条件给出，不要求特定表达式拼写。不能广播到多个位置，也不能在等长、缩短或可能回绕时把这份拥有值当作已被接收。原前缀及子资源保持，新增拥有内容只移动一次，最终仍按元素逆序沿原静态释放函数结束。普通函数交回的拥有dataset可继续作为下一次合法resize输入，真实子资源来源沿当前函数的原输入交接；旧逻辑绑定移走不表示交付存储已经结束。空原序列的共同模型取自这一新元素；其他输入合流旧新所有可能元素的共同nonzero／nonempty／index、活动载荷、CPU／GPU位置及借用来源，不以最后写入替代原元素合同。含view元素的resize增长及批量增长仍是独立缺口；本节不扩大静态F构造入口。

resize的原输入在后续length／fill求值到实际存储采用前保持同一作用域拥有；已求值的拥有fill从共同模型捕获前保留实际F与子资源，直到原存储调用成功采用。这些步骤真正失败时，由原Scope结束未采用fill、原输入及实际元素；成功则移出原持有关系，只交给返回的新Data一次。Frame继续唯一拥有原Dataset和CPU存储，resize直接借用该存储，扩容申请失败时原内容、长度和域关系仍保留；原Vec按实际容量增长／缩短并在新增范围填值；不新建资源登记、容器服务、存储镜像或回收链。缩短撤去逻辑尾部，底层容量可供随后增长复用，最终释放仍由原所有者负责，不宣称每次缩短都向操作系统归还内存。旧前缀／Count域关系在实际存储所有者处撤回。新返回句柄的SSA身份表示新的存储版本，旧长度或下标域证明不能套用到新值；这不是另一个运行时资源身份。

read／write／inout dataset借用不取得扩缩容权；inout拥有聚合仍按原规则保留容器位置，允许其独占拥有内容移动、调整并在返回前归还，不把两种借用混为一谈。元素游标、显式view及静态释放函数等真实后续消费者仍按原借用／最后消费者关系检查。GPU驻留先由拥有者显式download，再调整CPU存储；操作不隐式下载或等待其他工作。嵌套拥有字段、普通返回、Result／try、开放泛型及无源码模块／归档使用原所有权和资源位置合同，不能将外部地址当作独立所有者。

资源C入口`blink_resize(output,frame,handle,count,fill_address)->opaque_error*`；空指针表示成功并将原槽位实际句柄写入output，非空错误不写output。错误由调用者唯一拥有，文本通过blink_error_message借用到该错误销毁，最终由blink_error_destroy消费；它不借用Frame。fill_address仅在本次同步调用借用声明布局下的实际分量，Frame只读取这些分量，不能读取未初始化的填充；拥有内容的单次接收与最后释放仍由原编译期资源树交接。唯一Frame创建、解释和释放责任保持，不形成第二份拥有表或释放链；普通C值接口及view机器表示保持。完整C签名和失败合同见[NativeInterop](NativeInterop.md#共同frame-c-abi的错误与所有权)。

本范围是普通值动态存储与拥有值单项末尾增长基础；拥有元素子资源的完整长度关系／批量增长、宿主持久共享view图的B构造、递归容器、完整字符串、异步／远端和BLEngine实际替换仍按Issues保持开放。当前u32长度不是64位CPU或语言长期索引上限的替代规格，不将已有原生结果当作完整容器库或性能达标。

```b
dataset<u32> values=dataset<u32>(0);
values=resize(values,65,7);
values[0]=19;
values=resize(values,100,11);
// 原65项保留；新增35项为11，values[0]仍为19。
values=resize(values,1,0);
println(values[0]);
```

## CPU拥有dataset的尾部取出

`take_last(data,output)`在普通CPU流程中消费完整初始化、已证明非空的拥有`dataset<T>`，将最后一项交给同型可写位置output，并返回长度减一的动态形状拥有dataset。输入和输出位置按源码顺序各求值一次，输出可以是局部变量、记录字段、固定数组位置或CPU dataset元素；输出本来未初始化时由本次写入初始化，已经拥有的旧内容沿原覆盖职责结束。非空前提由原长度及索引关系证明，不能证明时报B137；借用输入、只读输出、已移动来源、活动借用冲突及类型不符继续沿原规则拒绝。

尾值是原实际元素的完整值与拥有元数据，不复制子资源，也不调用被交出尾值的静态F。普通函数可直接交回本地集合已经取出的自包含拥有尾值，剩余前缀在原函数结束；已交付子资源按其真实来源及所选位置命名为输出，不要求已经结束的本地父集合继续存活。拥有值经普通B函数或泛型继续按值转交时，view可用性沿原来源查询；实际结果或环境中的可用资源在当前路径上持有同一精确子资源时，查询在该资源处结束，不再要求旧父容器存活。外部view仍归自己的真实来源，不能借一份无关拥有资源延长寿命；按值dataset形参以原ResourceParameter表达资源来源，机器句柄／length及物理位置仍沿原ABI与内存效果交接；拥有元素物化直接交接本次真实读取的数值Proof，共同范围沿原函数及选支合同交给实际调用者；它不表示不同元素数值相等，空view与未知非空前提仍按原索引规则拒绝。模块来源合同中的选定下标当前交接常量、可映射的直接输入及原元素作用域序数；本地计算选择的数值关系尚未沿来源合同交接，见Issues C-020.1.6.5.4.3，未知下标或同一父字段不证明同一实际元素。这是当前合同能力缺口，不限制合法动态选择的语言目标。原Frame只缩短逻辑长度，原剩余前缀和尾值此后各归其实际接收者，分别在最后真实消费者结束后释放。缩短复用原resize入口且不读取填充值，不生成占位元素、运行时拥有位、资源副本或第二释放链；容量仍可复用，不承诺立刻归还操作系统。返回的新SSA身份表示同一Frame槽位的新存储版本，旧下标许可不能套到新值。

含资源、活动变体及地址／view的载荷继续携带原Data.elements、布局、位置与借用合同；非空或非零等共同元素要求、实际子资源寿命和外部ABI责任保持。输入已交给本表达式后，若输出位置求值经return／try提前退出，原临时拥有者保持到该出口的实际清理完成；没有发生的尾读取、输出覆盖和缩短不会执行。独立模块与归档消费已编译代码和同一资源合同，不重读实现；本能力不代表任意位置挖空、批量取出、递归容器或完整资源阶段。

## 普通值动态序列库与数值来源

正式B模块`blink.sequence`位于`library/blink/sequence.h`和`.b`，通过`-I library`装入。接口、使用方式与失败返回见[标准库说明](../../library/README.md)。它直接以拥有`dataset<T>`表达序列，本节完整库操作的采用范围为可复制、不含拥有资源或view的普通存储元素；pop通过下节take_last交接普通值或完整拥有尾值，成功返回剩余前缀与唯一尾值，空输入交回原拥有者；拥有元素push及其他完整库操作的采用仍独立验证，不由核心resize的单项能力代称；空序列直接构造`dataset<T>(0)`，清空直接使用`truncate(data,0)`。`get`／`set`提供动态下标的可失败访问；`push`／`pop`、`insert`／`remove`、`extend`、`truncate`沿原所有权与resize组合，不另存长度、容量或资源身份。

修改型操作按值消费输入，成功交回新拥有值；预期的空序列、非法下标或长度溢出沿声明的Result交回原拥有数据。`get`只读借用，`set`独占内容借用，`extend`只借用尾序列且不能与被移动的输入重叠。长度上限按当前u32表示表达；分配和原生执行失败仍由原运行库处理，不把它们伪装成库中的下标或长度错误。CPU位置、初始化、实际view／游标消费者与GPU显式传输继续沿原规则。

编译期无符号索引关系现在保留不回绕的和、上界距离及剩余长度：例如`cursor in index..size-1`的`cursor+1`，以及确认`tail.length <= 4294967295-begin`后拼接的`begin+cursor`。数值快照与资源存储版本分开；resize保留真实数值关系，但新存储不能接受旧版本的下标许可。普通函数返回模型中未知的来源继续未知，不能从合流模型残留的某一分支寄存器推断真实返回长度。它不是任意整数证明器，一般while回边不变式仍未交付。

dataset声明、重赋值及拥有字段投影统一消费原Returned求值；移动、部分移动与最终释放仍归原资源树和Frame。数值关系进入原模块合同。库的开放泛型源码可按实例编译；已编译模块只交付实际导出的实例，不提供二进制泛型模板。拥有元素子资源的完整长度关系／批量增长、递归容器、完整字符串、异步及BLEngine业务采用继续开放。

## AMD64 COFF原始结构读取

`blink.windows.coff`以同一B算法提供当前AMD64常规COFF的原始结构读取，接受机器号0x8664且无可选头的对象字节。普通B与原对象／物理代码事实读取者共用该模块。原始节名字段保持其至多8字节的现场表示；符号名沿原短名或字符串表读取，不规范化字节或保证UTF-8。归档、PE、B对象语义、模块合同与加载关系继续沿各自原入口处理。

这是原始C ABI跨度接口。`ReadBytes { address bytes; u64 length; }`借用调用者实际输入；其他Read记录中的跨度仍指向同一输入，不是拥有容器或经过B借用检查的view。调用者在最后读取结束前保持输入地址、长度和内容，headers／symbols及其派生表只与原输入配套；section索引在headers.length/40内，symbol索引按table.count及返回的next跳过辅助记录。所有函数同步完成，不保留地址、分配Frame、生成另一目录或建立输入身份服务。`next_export`只更新调用者自己的索引。

| 接口 | 实际结果 |
|---|---|
| `headers(input)` | `ReadRange`，原COFF节头表跨度及error |
| `build_inputs(input,headers)` | `ReadBuild`，原.blbuild字节、present及error，字段内部由原构建清单所有者解释 |
| `symbols(input)` | `ReadSymbols`，原符号起点／计数、字符串表跨度及error |
| `symbol(input,table,index)` | `ReadSymbol`，名字原字节、节号、值、存储类、函数标记、辅助记录、COMDAT关联字段、next及error |
| `next_export(input,table,index)` | `ReadRange`，按原顺序选出storage=2且section>0的定义名，跳过辅助记录；结束为空跨度，索引是inout u64 |
| `section(input,headers,index)` | `ReadSection`，原节名字段、物理载荷、flags、重定位起点／计数和zero_fill |
| `relocations(input,offset,count,flags)` | `ReadRelocations`，按原溢出标记交接实际表起点与计数 |
| `relocation(input,offset,index)` | `ReadRelocation`，原节内offset、符号表索引、kind及error |

error为0表示本次结构读取成功；1为当前机器／可选头不匹配，2为字节范围截断，4为符号字符串偏移错误，5为符号名缺少结尾，6为溢出重定位计数错误，7为辅助记录越过符号表。调用者按实际结果使用后续字段。原未初始化节在没有物理载荷时保持bytes.length=0，并以zero_fill交接其声明长度，不伪造已存在的零字节。实际字符串转换和最终拥有容器仍归真实消费者，原错误及尚未覆盖的格式扩展如实保留。

## UTF-8／UTF-16编解码、标量长度、借用／拥有切片与整段转换

`blink.text`以普通B函数提供`decimal_u64(u64 value)`和`decimal_i64(i64 value)`。返回独立拥有的CPU `dataset<u8>`，内容是有效UTF-8的十进制数字，负值带前导减号，零为单字节0。长度只覆盖实际数字及符号，前后缀、换行、终止NUL与输出目标由调用者决定。函数覆盖完整u64／i64范围，沿原dataset规则分配和释放。

`hexadecimal_bytes(read dataset<u8> data)`返回`result<dataset<u8>,u32>`，按输入顺序为每个字节写出两个小写ASCII十六进制位，包含高位零。空输入得到空序列，不增加前缀、分隔、换行或终止NUL。成功结果独立拥有，输入只读借用到同步返回；改写或释放输入不影响结果。输出需要两倍字节数，若超出现行dataset的u32长度表示，Err保留原输入长度；这是结果容器的实际表示边界，原分配／执行失败继续沿运行库处理。

`blink.text_core`以共享普通CPU算法同步写十六进制字节。原生`write_hexadecimal(address input,u64 count,address output)`借用count个输入字节和可写两倍字节的非重叠输出区，完整写出ASCII，不持有地址或Frame。普通B接口在原Frame分配最终序列；编译查询在原String分配最终文本区，同步写完后提交其有效长度，不另造编码副本或调用宿主格式化。

正式B模块`blink.unicode`以普通CPU函数实现`decode_utf8`、`decode_utf16`、`encode_utf8`和`encode_utf16`，以及`to_utf16(read dataset<u8>)`和`to_utf8(read dataset<u16>)`。标量解码Result交回value及实际next位置，或含原位置与原因的DecodeError；UTF-8位置以字节计，UTF-16以逻辑u16代码单元计。标量编码将合法u32交回按实际单位数区分的固定载荷变体，非法值在Err保留。

`blink.unicode_scalar`共享普通CPU纯值的Unicode标量判断及UTF-8字节位布局。`is_scalar(u32 value)`判定合法Unicode标量；`utf8_units(u32 value)`只消费已验证的合法标量，交回`Utf8Units {u32 length;u32 bits;}`，length为实际1至4字节，bits按低字节在前保存UTF-8单元。它不持有文本或Frame，不替代`encode_utf8`对任意u32的Err／Ok合同；原UTF-8接口、整段转换及源文本解码消费同一位布局，非法输入不依赖该纯值函数交付编码结果。独立Unicode对象的实际依赖包含该纯值模块，归档和分立对象命令见标准库README。

scalar_length_utf8(read dataset<u8>)和scalar_length_utf16(read dataset<u16>)复用原严格decoder，从零到末尾取得Unicode标量数，返回Result<u32,DecodeError>；空输入为Ok(0)，第一个错误原样保留绝对index和reason，不交付部分计数。NUL、BOM、组合标记各按标量计数，UTF-16合法代理对计一个；已有dataset.length仍为字节或u16代码单元数，不表示字素簇。长度统计只读借用输入，不分配或复制文本、保存计数缓存或改变输入所有权。

slice_utf8(read dataset<u8>,u32 start,u32 end)和slice_utf16(read dataset<u16>,u32 start,u32 end)返回Result<dataset<原单元类型>,SliceError>，以半开Unicode标量区间[start,end)构造独立拥有序列，保持所选原始代码单元；合法空区间及末尾空区间成功。整个输入严格解码，第一个非法编码优先以SliceError::Decode保留原绝对位置与原因，所选区间之后的非法编码也不忽略。完整文本合法时要求start<=end<=标量长度，否则SliceError::Range的ScalarRangeError保存原start／end和实际标量length。输入只读借用，成功输出可独立修改、移动和释放。实现直接消费borrow_utf8／borrow_utf16校验和选择后的原单位view，成功后按实际单元数一次分配并同步复制；Decode或Range不先生产拥有输出。临时view只保持到复制结束，返回结果不借用输入，存储沿原Frame与最后消费者释放。输出单元不超过原输入，不重编码、不另建文本状态或边界表。它们不按字素簇切割或改变原输入所有者，分配／执行失败继续按原运行库处理。

borrow_utf8(read dataset<u8>,u32 start,u32 end)和borrow_utf16(read dataset<u16>,u32 start,u32 end)以同一严格解码及SliceError规则分别返回Result<view<u8>,SliceError>和Result<view<u16>,SliceError>，按Unicode标量半开区间借用原UTF-8字节／UTF-16代码单元；合法代理对计一个标量且保持两个原单元，合法空及末尾空区间成功。整个输入严格解码，所选范围之外的第一个错误仍优先于Range，Decode保存原绝对单元位置／原因，Range的length仍为实际标量数。不分配、复制、重编码或保存边界表；原dataset继续唯一拥有存储，返回值及独立模块／归档沿普通view来源与最后消费者交接。活动借用中不能改写、替换、传输或释放原存储；错误结果不保留成功分支的活动借用。

整段转换只读借用输入，从零到末尾严格解码，Ok交回新的拥有dataset；空输入返回空序列。TextError的Decode分支保存原DecodeError，Sequence分支保存序列操作原SequenceError；前者按原输入定位第一个错误，后者沿当前u32长度表示交接实际溢出等序列原因。失败的部分输出由原Frame随退出释放，输入与原所有权保持。实现复用已验证的标量解码、同一编码载荷和blink.sequence::push／resize，不增加文本状态、容量管理或释放链；分配／执行失败仍沿原运行库处理。

接受U+0000至U+10FFFF中的非代理标量；NUL、BOM、未分配码点与noncharacter保持原值。非最短UTF-8、代理编码、越界、非法续字节、孤立／错误UTF-16代理及截断保留严格错误，不替换字符、不隐式跳过或追加NUL。具体类型、错误位置及源码／对象／归档用法见[标准库](../../library/README.md#blinkunicode)。标量接口可在CPU并行域使用，整段拥有转换在普通CPU流程执行；普通Result／变体、原资源ABI和最后消费者规则继续适用。

普通源码文本字面量以既有固定UTF-8字节数组交接，见“文本字面量”；与dataset之间的逐项构造及编码转换保持显式。完整动态字符串值、一般OS字符串地址／生命周期及BLEngine业务采用仍按C-020.6.2.1保持开放；这些接口不表示完整文本库或GPU转换已经交付。

## 不回绕加法的输入快照与偏移

在已有无符号不回绕条件成立时，`base+常量`的结果保留相对真实base输入快照的精确偏移；连续加法组合偏移，并向已有顺序关系交接base<=结果，正偏移时为base<结果；不同正偏移分支仍可保留共同顺序。不回绕条件不成立时仍遵守原u32回绕语义，不能取得数学整数的上界关系。以`resize(data,size+1,fill)`产生新长度后，原size快照及其向下距离可以证明新存储中的下标，比较同一base的两个偏移也保持实际大小关系。旧资源许可仍不能跨存储版本沿用。

该关系与原Proof／Domains一起交接，模块合同保存实际base及常量距离，机器值和Frame保持。独立模块只按公开真实输入来源替换快照，不读取私有源码，不把丢失来源猜成其他输入。一般while入口与回边的数值不变式仍按C-005.3研发，本项单次表达式关系不替代循环推导。

## 宽数值与当前模块合同
普通u64／i64、返回范围与条件输出中实际使用的完整标量常量按真实位宽编码；类型和位模式共同构成合同，不能因高32位相同或不同而截断、混用类型或猜测条件。主线已有的条件输出、用户拥有值、不可变view、显式resize与普通值序列沿同一类型与资源关系组合。

模块资源保留原不透明Frame、同步真实完成及最后消费者释放，改变的是实际宽标量、原生映像和C Value载荷编码。不能把重编成功等同于版本迁移或消费者已经采用。handle保持不透明身份，u64不授予地址或句柄操作；dataset长度／下标、const形状与枚举判别字保持自身32位合同。

address值与usize／isize按原生地址章节交接；外部地址CPU位置读写按原生地址章节使用；自有地址的完整初始化结果与寿命交接、回调、通用字符串、动态拥有元素与递归容器、浮点原子、一般while不变式、异步及热更新仍待实现。GPU64位tile的已实现范围与当前原生缺陷见块内原子章节。f64、窄整数和数值排序按本文对应章节的当前范围使用。

## 循环整数位移、赋值与有限反向不变量

调用的效果分析按实际调用种类使用合同：普通原生调用可由其闭合合同精化可能写入，整域调用继续保留参数写及尚未闭合的外部内存写可能性，由其整域接口交接实际数据结果；不能把整域调用当作只读，也不要求另一份普通函数实例。CPU域体的C ABI写入仍遵循原效果和访问规则。普通while及有界for循环沿同一数值效果处理确定位置的u32分量和本地拥有dataset长度；存储分量描述`+=常量`、`-=常量`、同位置的`x=x±常量`、独立赋值与`x=c-x`的有限反向关系，dataset长度赋值复用原独立赋值及路径合流。resize同长度或缩短时，已有长度下界可以证明填充值没有写入保留前缀；增长则把实际新增元素事实交给真实结果模型。位移的顺序组合保留累计位移及中途极值，只有初始入口和全部继续路径共同保持、且实际条件足以排除中途回绕的上界或下界才交给原Proof。反向常量c沿原写集合确认为循环中不变；同一反向连续两次恢复原值，因此初始范围与一次反向范围的并集覆盖任意次数的实际继续边，按原u32整数回绕计算。if／match按可能路径合流，continue与自然尾部形成回边，break／return结束该路径；该范围由原下标及非零检查消费。未知调用、条件改写、动态候选别名及改写该位置的嵌套循环继续沿实际效果处理，不能由未知来源生成范围。

递减游标可沿入口数值快照保留上界；不回绕递增位置把入口精确偏移放宽为相对下界，保留值不小于原来源加原距离的关系。沿同一实际来源求差时可据此证明正差与非零，不把递增后的值仍当作入口精确值。同宽u8／u16／u32／u64的显式严格顺序也可交接正差：在end>begin成立的分支中，end-begin至少为一，上界按实际标量位宽确定；减法消费左值已有下界或右值已有上界时均匹配实际分量和来源快照，非严格顺序或真实改写不取得非零结论。一般无符号数值关系与当前u32资源索引ABI分开，有符号差值不据此成为正数。同位宽符号往返保留原位模式关系，缩窄转换不继承原域。当前值继续使用真实本轮读取身份，不继承入口常量、相等来源或变体选择；条件失败与break出口沿原循环所有者合流。资源存储版本、初始化、借用及最后消费者职责保持，数值上界不许可访问已更换的存储。固定形状及当前u32长度ABI的dataset／view／tile索引前提同时交接其数值上界，算术可以消费合法索引的初次加一不回绕；该推论依赖实际长度表示，不规定永久长度或索引宽度。该范围不承诺任意非线性、可变步长、动态别名或跨位置循环不变式；无法静态确定的下标仍可用正式库可失败访问表达。机器ABI保持。

## 相同借用关系的条件合流

两个分支的完整借用关系相同时，编译期合流直接复用该关系；完整相等包括来源及其成立条件、循环输入和调用身份。条件表达式仍按源程序求值，实际控制流及副作用保持。来源集合相同而边条件不同的借用仍沿原条件交接，最后一个真实消费者与资源释放责任保持。

来源边首次写入保存原共享条件图，同一来源实际合流时再作原有规整；单条边不预先展开为路径项组合。同来源规整先用原事实子集决定候选去留，只有未据此决定时才执行原蕴涵查询；原事实规则和条件结果保持。调用输入的身份、域、位模式及原Fact转换由同一Proof所有者交接；Clause沿原同次共享图一次投影所有实际条件根，临时映射随最后根交接结束。私有数值快照的原required查询仍将必要数值界写回各条件根。数值代入只为当前Proof实际引用的标量身份建立临时绑定。已解值交给原调用投影，未替换的身份保持自身。调用合同与已解值代入共用标量及条件投影。已解值代入由原完整值集合直接映射实际借用及已有源地址，缺少源地址的分量可从绑定值继承地址。循环数值求解沿当前槽身份从入口和继续边读取该值。条件投影按原输入集合取得完整绑定。真实数值／资源Proof继续处理自己的范围、地址／view、入口条件继承及非零消费；纯Condition直接交接各自真假根，不为随后丢弃的临时数值Proof计算非零。一般两根不必互补，原事实、未知输入和浮点语义保持，不新增图、缓存或第二规则。来源边的调用绑定直接消费原Condition::at_call已投影的正根，再交给原insert_when形成最终边的完整条件；不在只取正根的交接中先生成随即丢弃的补集。一般Condition两根不必互补，真实私有布尔输入、未知／不可能来源及存储版本仍由原投影解释，最终边的两个根继续由同一边所有者持有。条件语义、模块编码、收敛和资源消费者职责保持，不以数量上限截断关系。

## 普通返回值的条件借用来源

普通CPU函数在多个返回点合流时，view及包含view的普通记录、固定数组和变体沿每个真实返回路径交接来源条件，与拥有聚合和dataset输出使用同一返回所有者。调用者以本次实参快照及后续实际分支判断哪些存储仍被借用；可以在已证明只借用另一存储的路径中修改、移动或显式传输本存储。来源条件不被无条件候选集合代替，也不以参数名判断独立。

条件后来改写不改变既有view的来源；仍将消费的view、调用／循环及用户静态释放保持实际来源到最后消费者完成。不能证明互斥时继续保留所有可能来源，私有未知条件不产生公开许可。本节使用原Proof／Borrow、返回合流与模块合同，没有运行时借用状态、资源副本或额外检查；view机器表示保持。该范围不承诺任意布尔／数值相关性的完整推导。

借用来源的重叠判定与各路径共有事实分别查询：实际借用拒绝入口直接消费原共享Clause图，沿同一路径按原矛盾关系合取事实，已不可能的备选结束；不能因两个析取分别没有共有原子，就将互斥来源当作可能重叠。普通范围与来源关系继续消费原共有事实，不为局部推导枚举路径。重叠查询不物化所有路径组合，不新增运行时借用状态、公开条件类型或模块编码，也不据此承诺任意算术／布尔关系的完整决策能力。

## parallel阶段的共享只读访问
计算函数的inout dataset参数可以在一个parallel阶段只作共享读取；前一阶段完成写入，不要求把存储复制或包装成另一份read数据集。已有范围证明的直接索引、for只读游标及现有read dataset函数调用均使用原存储。unsafe_read只解除范围证明，不解除权限、初始化或并发访问约束；write与atomic参数不能因此成为普通共享输入。

普通写入继续只允许本invocation的当前index()元素。同一阶段对同一存储中的重叠区域同时具有普通跨元素读取与写入时，编译期报B166，先读后写与先写后读均适用；声明布局及实际候选位置已确定的不相交字段可以分别读取和写入，动态数组／向量与嵌套记录沿同一位置映射判断；后续阶段可以重新取得原inout能力。read形参在被调函数内保持只读，不能借转借取得写权。tile输入快照、显式atomic和append沿各自原有的同步／独占规则处理。append输出／计数仍不能与同阶段普通读取重叠（B221），包括通过read函数转借的当前元素读取。访问区域在原位置降低时交给当前Builder，不等IR丢失字段关系后再重建；常量选择只保留被选分支，动态选择合入两侧实际区域。原阶段资源依赖保持，资源、CPU／GPU完成与释放仍归原所有者，不增加运行时检查、数据镜像或设备提交。

共享只读阶段不改变完整异步借用和版本迁移的未完成状态。

## 窄整数与精确字节布局

u8／i8为8位整数，大小与对齐均为1；u16／i16为16位整数，大小与对齐均为2。有符号值采用二进制补码。普通记录、固定数组及外部ABI消费原Ty的字节布局，不将窄值替换成u32字段或以打包宿主容器代理。

窄整数与原整数使用同一运算规则：加、减、乘、取负及整数转换按目标位宽取低位；8位整数的移位量取低三位，16位取低四位，左移结果按实际位宽截断，无符号右移补零，有符号右移补符号位。位宽倍数、大计数及负的有符号计数均按同一低位规则解释。扩大有符号整数先按来源位宽符号扩展，无符号整数零扩展。不同固定数值类型继续显式转换；原未固定字面量规则保持。浮点转整数的有效输入范围按实际目标类型确定，不钳制NaN、无穷或越界值。当前仅增加整数标量，不增加窄浮点、窄原子或新的索引类型。

Windows x64原生C ABI使用int8_t／uint8_t／int16_t／uint16_t，普通C／Rust记录按原布局传递；寄存器参数和返回只消费实际低位，不能要求外部函数初始化未定义高位。CPU内存只读写真实1／2字节。GPU声明类型、实际设备的整数与存储能力及字节地址由原SPIR-V／Vulkan所有者承担，不能用非原子整字读改写破坏同一字中其他独立并行位置；共享／私有编译存储和真实资源布局分别落实。

窄值可用于普通变体、原始记录复制／append、内置与自定义数值集合、tile及生成C／Rust算法绑定，各操作沿本节真实字节布局和相应算子范围执行；具体原生及消费者采用记录只保存在双台账。
局部存储使用原声明Ty作为locals中的布局来源，局部编号只标识真实逻辑分量，不按四字节填满空槽。实际地址消费者保留该声明的布局，NativeAllocation及SPIR-V消费同一类型与字节位置；非寻址局部值仍可分配寄存器。GPU私有数组按真实声明分量选择实际存储单元，Vulkan声明shaderInt8／shaderInt16及storageBuffer8BitAccess／storageBuffer16BitAccess；push值从原只读32位字中提取，不要求额外窄push能力。原布局、位置、借用和最后消费者继续各归原所有者，不另建运行时表、字节容器或布局镜像。
变体仍使用原判别字与重叠载荷；内部u32载荷分量按真实字节范围组合和提取，包含相邻窄字段及声明填充，不能向下取整后丢失位置。完整bool／view分量继续保留原Proof关系，不以位编码替换借用来源。该子范围的实际验证另见双台账。
内置数值／按位集合沿相同位宽与有符号规则组合。原范围摘要及GPU共享树保留实际需要的完整物理u32单元，源／目标字段使用真实字节位置；摘要填充不成为额外数值。CPU内核必须按字段实际宽度读写，归还结果不能覆盖相邻字段。普通／分段扫描及归约的当前实现、原生范围与剩余项继续分别记入双台账。
前缀计数仍是独立形状1资源中的一个u32字段；计数身份包含该字段的真实字节位置。记录中的其他普通字段不改变计数职责，集合、clear_prefix及append只操作声明的计数位置，不能要求整条记录只含这一个分量。
原始compact／稳定排序按原Ty完整字节布局复制，包含填充和数值位模式；排序键身份继续保留真实字节位置。append构造实际记录值时只写声明的真实分量，保持目标填充及相邻记录，不能用宽存储覆盖窄字段。CPU／GPU分别沿原机器指令与原生存储操作落实，不新建运行时拷贝所有者。
自定义集合按原Ty字段位段交接真实标量；同一物理摘要字中的窄字段不得互相覆盖。投影与直接值输入、普通与分段扫描／归约共用原ValueCode和有序合并规则；共享容量必须报告实际物理单元，不以载荷字节数低报。编译器私有摘要填充不成为用户字段，实际目标仍只写声明的真实分量。

tile输入、发布及动态读取消费原Ty的实际字段字节。CPU使用原u32共享格，GPU按真实位宽使用u32／u64 Workgroup平面，由B发射实际地址和窄字段读取。固定字段直接取得编译期移位量，同格字段保留最后真实消费者，发布只写优化后保留的分量所覆盖的物理字。CPU跨阶段私有流和GPU集合树按实际存储字分配，数据载荷不扩大。8／16位tile集合沿实际模算术、有符号比较及位运算，32／64位值仍沿原布局；普通tile保持只读，显式tile_atomic沿u32／i32／u64／i64及实际目标能力执行。f64位复制保留完整位模式，GPU浮点运算继续要求真实目标能力。

普通CPU函数，以及CPU／GPU元素阶段的普通／cooperative函数，可对普通标量、记录、固定数组、向量、enum／Result及其view字段声明read，实参与声明类型一致。设备阶段按原目标与参与范围内联，普通GPU原生调用不由本项新增。已有位置直接按真实布局借用，多个只读参数可以重合；不相交独占位置仍按原规则允许，重叠独占、未初始化读取和活动view所指存储的写入／移动继续拒绝。调用者表达式产生的普通值及常量使用原临时存储；窄字面量仍按既有显式转换规则书写。

参数只读权限同时约束字段和动态元素；独立函数检查与实际调用均绑定原只读位置，read不能转发为write／inout。返回view沿原Proof交接真实外部来源及长度关系，不能因参数地址改变而丢失来源。实参按源码顺序求值，实际拥有父值保持原退出释放顺序；无源码对象和归档保存同一输入内容与返回关系，不增加机器参数、聚合镜像或运行时登记。原view机器布局保持；一般借用返回、可写view、其他目标普通调用、异步和远端继续按实际缺口记录。
