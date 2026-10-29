// 简体中文文案（默认语言，作为**键的唯一事实来源**）。
//
// 组件内禁止出现中文字面量；一切展示文案走本字典。

export const zhCN = {
  app: {
    title: 'PasswordMS',
    tagline: '本地优先 · 零知识密码管理',
  },
  common: {
    ok: '确定',
    cancel: '取消',
    close: '关闭',
    save: '保存',
    delete: '删除',
    edit: '编辑',
    copy: '复制',
    copied: '已复制',
    copied_countdown: '剪贴板将在 {n} 秒后清空',
    confirm: '确认',
    back: '返回',
    search: '搜索',
    clear_search: '清空搜索',
    minimize: '最小化',
    maximize_restore: '最大化 / 还原',
    close_window: '关闭窗口',
    loading: '加载中…',
    none: '暂无',
    yes: '是',
    no: '否',
    retry: '重试',
    add: '新增',
    create: '创建',
    rename: '重命名',
    remove: '移除',
    clear: '清空',
    unknown: '未知',
    expires: '有效期',
    seconds: '{n} 秒',
    minutes: '{n} 分钟',
    days: '{n} 天',
    items_count: '{n} 条',
  },
  lock: {
    title: '解锁保险库',
    password_label: '主密码',
    password_placeholder: '请输入主密码',
    unlock: '解锁',
    create_new: '创建新保险库',
    busy: '正在解锁…',
    error: '解锁失败，请重试。',
    show_password: '显示密码',
    hide_password: '隐藏密码',
    // D-55 锁屏页「从备份恢复」：入口按钮、覆盖警告、空密码提示。
    restore_button: '从备份恢复',
    restore_confirm_title: '从备份恢复',
    restore_confirm_body: '将使用所选备份替换本机当前保险库，原库会自动备份为 .bak 文件。确认继续？',
    restore_password_required: '请先输入主密码（备份创建时的主密码），恢复后将用它自动解锁。',
    // D-113 P2：快速解锁按钮（仅本机已开启且平台可用时出现）。
    biometric_unlock: '使用 Windows Hello 解锁',
    biometric_waiting: '等待 Windows Hello…',
    // §23：锁屏按钮同样按平台取词。
    biometric_unlock_android: '用指纹解锁',
    biometric_waiting_android: '等待指纹验证…',
  },
  create: {
    title: '创建保险库',
    master_password: '主密码',
    password_label: '主密码',
    confirm_password: '确认主密码',
    confirm_placeholder: '再次输入主密码',
    password_placeholder: '至少 {n} 位',
    hint: '8 位以上，建议混合大小写、数字与符号',
    mismatch: '两次输入的主密码不一致。',
    too_short: '主密码至少需要 {n} 位。',
    weak_hint: '主密码较短，建议使用更长的口令；继续请勾选确认。',
    // D-63：勾选行拆成「前缀 + 免责声明链接 + 与 + 用户协议链接」——
    // 两个链接是 checkbox 所在 label 的**兄弟节点**，点击只开法律弹窗、不切换勾选。
    acknowledge_prefix: '我已阅读并同意',
    disclaimer_link: '免责声明',
    acknowledge_middle: '与',
    terms_link: '用户协议',
    submit: '创建并解锁',
    warning_body: '主密码无法找回。请妥善保管，遗失将无法恢复保险库数据。',
  },
  nav: {
    items: '全部条目',
    favorites: '收藏',
    health: '密码体检',
    trash: '回收站',
    settings: '设置',
    sync: '同步',
    lock: '锁定',
    // 移动端底部导航的**短标签**：78px 宽的格子塞不下「全部条目」这类长文案。
    tab_items: '条目',
    tab_health: '体检',
    tab_trash: '回收站',
    tab_sync: '同步',
    tab_settings: '设置',
    // 移动端顶栏「⋯」溢出菜单与左侧抽屉/返回按钮。
    more: '更多',
    open_folders: '打开分组',
    language: '语言',
  },
  folder: {
    all: '全部',
    ungrouped: '未分组',
    new: '新建分组',
    name_placeholder: '分组名称',
    rename_title: '重命名分组',
    rename_confirm: '保存',
    delete_title: '删除分组',
    delete_body: '该分组下的条目将被移出分组或一并移入回收站，请选择处理方式。',
    delete_to_ungrouped: '移出分组',
    delete_to_trash: '移入回收站',
    empty: '还没有分组',
  },
  item: {
    new_login: '新建登录',
    new_note: '新建笔记',
    empty: '还没有条目',
    no_results: '没有匹配的条目',
    favorite: '收藏',
    unfavorite: '取消收藏',
    copy_username: '复制用户名',
    copy_password: '复制密码',
    open_url: '打开网址',
    delete: '删除',
    delete_perm: '彻底删除',
    restore: '还原',
    created: '创建于',
    updated: '更新于',
    rev: '版本',
    type_login: '登录',
    type_note: '笔记',
    type_attachment: '附件',
    // D-88：登录项一键转安全笔记（A1 映射：原字段全部并入正文，一样不丢）。
    convert_note: '转为安全笔记',
    convert_note_title: '转换为安全笔记？',
    convert_note_body:
      '备注会成为笔记正文；网址、用户名、密码、TOTP 与自定义字段会原样并入正文尾部（不丢失）。此操作不能自动撤销。',
    convert_fields_header: '——以下为原登录项字段——',
    convert_done: '已转换为安全笔记',
  },
  // #531：上下文菜单（桌面右键 / 移动端长按）。
  menu: {
    pin: '钉住',
    unpin: '取消钉住',
    rename: '重命名',
    move_to_folder: '移动到分组…',
    move_title: '移动到分组',
  },
  // #531：拖拽手柄的辅助文本（行的 aria 补充，视觉隐藏）。
  dnd: {
    handle_item: '按住拖动可钉住或调整顺序',
    handle_folder: '按住拖动可钉住或调整分组顺序',
  },
  editor: {
    title_new: '新建条目',
    title_edit: '编辑条目',
    kind_login: '登录',
    kind_note: '笔记',
    kind_attachment: '附件',
    field_title: '标题',
    field_url: '网址',
    field_username: '用户名',
    field_password: '密码',
    field_notes: '备注',
    field_totp: 'TOTP 密钥',
    totp_placeholder: 'BASE32 或 otpauth://',
    custom_fields: '自定义字段',
    field_key: '字段名',
    field_value: '字段值',
    add_field: '添加字段',
    folder: '分组',
    favorite: '收藏',
    show_password: '显示密码',
    hide_password: '隐藏密码',
    reveal_title: '显示密码',
    reveal_body: '即将在屏幕上显示明文密码，请确认周围无人可见。',
    totp_now: '动态口令',
    save: '保存',
    cancel: '取消',
  },
  gen: {
    title: '密码生成器',
    length: '长度',
    uppercase: '大写字母',
    lowercase: '小写字母',
    digits: '数字',
    symbols: '符号',
    symbol_pick: '点选允许参与生成的符号（全不选 = 用默认安全集 + - _ . , :）',
    exclude_ambiguous: '排除易混字符',
    generate: '生成',
    copy: '复制',
    use: '使用此密码',
    need_one: '至少选择一类字符',
  },
  meter: {
    weak: '弱',
    fair: '一般',
    strong: '强',
    very_strong: '很强',
  },
  note: {
    title: '标题',
    body: '正文',
    placeholder: '在此输入笔记内容…',
    too_long: '正文超过 {n} KiB 上限。',
  },
  attach: {
    title: '附件',
    choose: '选择文件',
    import: '导入为附件',
    export: '导出',
    export_confirm_title: '导出附件',
    export_confirm_body: '导出后文件将以明文形式保存到磁盘，请确认存放位置安全。',
    size: '大小',
    too_large: '单个附件不得超过 {n} MB。',
    vault_full: '保险库已达 {n} MB 容量上限。',
    filename: '原始文件名',
    mime: '类型',
    // D-53：blob 未同步时的占位态。
    pending_sync: '附件内容待同步：内容尚未从其他设备同步到本机，同步后即可导出。',
    // D-60：详情页附件区（父条目内嵌 attachments）。
    add: '添加附件',
    remove: '移除',
    remove_confirm_title: '移除附件',
    remove_confirm_body: '附件「{name}」将从条目移除，其内容会在同步后从所有设备删除。此操作无法撤销。',
    section_title: '附件',
  },
  import: {
    // D-108（豆包审查 3.5-9）：CSV 表头读取失败不再静默重置。
    header_read_failed: 'CSV 表头读取失败：字段映射已留空，预览时将重新读取文件。',
    title: '导入',
    // D-73：原「选择导出文件」被主人读成「选本应用导出的文件」（本应用导出的是 .vault
    // 加密备份，根本不从这个入口导）→ 改成动作方向明确的措辞。
    choose_file: '选择要导入的文件',
    // D-73：「预览」不再因为未选文件而置灰（置灰+无说明 = 用户卡死），改为可点+提示。
    choose_first: '请先点上面的「选择要导入的文件」。',
    format: '格式',
    format_json: 'JSON',
    format_csv: 'CSV',
    preview: '预览',
    items: '条目',
    folders: '分组',
    skipped: '跳过',
    apply: '开始导入',
    done: '导入完成',
    mapping_known: '已识别列',
    mapping_custom: '自定义字段列',
    encrypted_hint: '不支持加密导出文件，请导出为未加密格式。',
    // D-72：支持范围 + CSV 列清单。10 个列名与 `src/lib/import.ts` 的 `KNOWN_COLUMNS`
    // 及 core 的 `import::bitwarden_csv::KNOWN_COLUMNS` 逐项一致 —— 改列名要三处同改。
    // `type` 取值那句必须留在界面上：用户最常问的就是「我这条为什么被跳过」。
    supported_hint: '仅支持 Bitwarden 未加密导出（JSON / CSV）。加密导出与其它密码管理器的格式需先转换。',
    template_hint: 'CSV 识别这 10 列：folder / favorite / type / name / notes / login_uri / login_username / login_password / login_totp / fields；其它列会保留为自定义字段。type 只有 login 或 1（登录）、secure note / note / 2（安全笔记）会导入，其余类型跳过。',
    // D-87：编码容错上线的说明。GBK / UTF-16 已能自动转换，但「另存为 UTF-8」仍是最稳做法
    // （避免个别生僻字在 GBK 与 Unicode 映射差异下出错）。
    encoding_hint: '用 Excel / WPS 编辑 CSV 后，建议另存为「CSV UTF-8」编码；GBK（ANSI）与 UTF-16 文件会自动识别转换。',
    // D-90：合并/查重结果如实上报（结果行追加显示，0 时不出现）。
    duplicates: '重复跳过 {n} 条',
    merged_folders: '合并同名分组 {n} 个',
    download_template: '下载 CSV 模板',
    template_saved: '模板已保存',
    // D-112：导入模板的本地化示例文本（结构在 core，文案随 UI 语言）。
    // tpl_login_notes 必须自带半角逗号（引号转义示范的组成部份），翻译时勿删。
    tpl_folder: '示例分组',
    tpl_login_name: '示例登录条目',
    tpl_login_notes: '备注里可以有半角逗号, 但整段必须用英文双引号包住',
    tpl_note_name: '示例安全笔记',
    tpl_note_body: '笔记正文写在这里',
  },
  health: {
    title: '密码体检',
    run: '开始体检',
    score: '健康分',
    logins: '登录条目',
    weak: '弱口令',
    reused: '重复密码',
    reused_group: '第 {n} 组',
    stale: '超期未改',
    suggestions: '建议',
    good: '密码健康状况良好。',
    // D-112：整改建议由前端按计数组装（core 不再出文案）。
    sug_weak: '有 {n} 条条目使用弱口令，建议改为 12 位以上的混合字符密码。',
    sug_reused: '有 {groups} 组条目重复使用同一密码（共 {n} 条），建议为每站设置独立密码。',
    sug_stale: '有 {n} 条条目超过 {days} 天未更换密码，建议定期轮换。',
  },
  trash: {
    title: '回收站',
    empty_confirm_title: '清空回收站',
    empty_confirm_body: '此操作不可恢复，将永久删除回收站内所有条目。',
    restore: '还原',
    purge: '彻底删除',
    purge_confirm_title: '彻底删除',
    purge_confirm_body: '此操作不可恢复，将永久删除该条目。',
    empty: '清空回收站',
    deleted_at: '删除于',
    expire_at: '到期',
    none: '回收站为空',
  },
  theme: {
    label: '主题',
    system: '跟随系统',
    light: '浅色',
    dark: '深色',
    to_light: '切换到浅色',
    to_dark: '切换到深色',
  },
  settings: {
    title: '设置',
    group_general: '通用',
    group_appearance: '外观',
    group_security: '安全',
    group_generator: '生成器默认值',
    group_health: '体检阈值',
    group_sync: '同步',
    group_about: '关于',
    // D-113 P2：生物识别快速解锁（`docs/08-biometric-unlock.md` §6.1 / §10）。
    // 文案必须如实说明「这是 Windows Hello，不是指纹」「能拿到本机登录凭据者可绕过」。
    group_biometric: '快速解锁',
    // §23：Android 上线后，卡片里的平台名词必须跟着平台走。用一个 `{noun}` 插值，
    // 而不是给每条文案都写两遍——否则「Windows Hello」会继续漏到手机上去（D-116 同类缺陷）。
    // §23：Android 侧的名词。用在 `{noun}` 插值的通用句里（如「本机尚未设置 {noun}」）。
    bio_noun: 'Windows Hello',
    bio_noun_android: '指纹',
    bio_title: 'Windows Hello 快速解锁',
    bio_desc: '用 Windows Hello 代替每次输入主密码。密钥只在本机、本 Windows 账户下可用，不进入备份与同步。',
    bio_risk: '注意：能拿到本机 Windows 登录凭据的人可以绕过 Windows Hello。本功能的安全上限取决于该账户的锁屏强度。',
    // Android 侧是另一套名词，也是**另一套风险模型**：这里不存在「拿到账户凭据就能绕过」，
    // 真正的约束是「新增指纹即让密钥永久失效」。照搬 Windows 那段话等于说错话。
    bio_title_android: '指纹快速解锁',
    bio_desc_android:
      '用手机自带的指纹代替每次输入主密码。密钥由系统安全芯片保管，只在本机可用，不进入备份与同步。',
    bio_risk_android:
      '注意：新增或删除指纹等生物识别记录后，已保存的密钥会永久失效，需要重新开启。这是系统的安全设计——防的正是「有人趁你不注意加一枚指纹」。',
    bio_state_on: '已开启（{date}）',
    // D-115：`state=ready` 只说明**封印文件**完好，不说明平台现在能用。Hello 被关掉时
    // 两者会同时成立，旧文案只说「已开启」就与下面的红字自相矛盾（主人真机截图）。
    bio_state_on_unavailable: '已开启，但当前不可用（{date}）',
    bio_degraded_hint: 'Windows Hello 关闭或未开通期间，快速解锁一次也用不了。重新开通后会自动恢复——已保存的配置不会被删除，也不需要重输主密码；要彻底撤销请点「关闭」。',
    bio_degraded_hint_android:
      '指纹不可用期间（例如把所有指纹都删了），快速解锁一次也用不了。重新录入后会自动恢复——已保存的配置不会被删除，也不需要重输主密码；要彻底撤销请点「关闭」。',
    bio_state_off: '未开启',
    bio_state_corrupt: '配置已损坏，请关闭后重新开启。',
    bio_state_stale_password: '主密码已修改，快速解锁已失效。',
    bio_state_stale_vault_key: '保险库密钥已更换（配对加入），快速解锁已失效。',
    bio_state_stale_enrollment: 'Windows Hello 配置已变更，快速解锁已失效。',
    bio_state_stale_enrollment_android: '设备的生物识别记录已变更，快速解锁已失效。',
    bio_state_stale_tampered: '快速解锁配置被篡改或损坏，已失效。',
    // D-114：`not_present` 曾被写成「本机没有可用的硬件」，但 Windows Hello 从未配置过时
    // WinRT 回报的正是 `DeviceNotPresent` —— 主人实测被这句话误导。文案改为只陈述
    // 「检测不到」+ 给出下一步，不再替系统断言「你没硬件」。
    bio_avail_not_configured: '本机尚未设置 {noun}。',
    // D-125/D-126：只录了便捷级（Class 2）。**不能说「尚未设置」**——
    // 用户的手机解锁明明一直在用；D-126 起 Android 文案一律不提「人脸」。
    bio_avail_weak_only: '本机只录入了「便捷级」生物识别：它能解开手机屏幕锁，但 Android 不允许用它保护加密密钥，所以本功能用不了。请先在系统设置里录入一枚指纹。',
    bio_avail_not_present: '未检测到可用的 {noun}（多半是尚未设置，不一定缺少硬件）。',
    bio_avail_hint: '请到「设置 → 账户 → 登录选项」开通 Windows Hello（PIN 或指纹），完成后切回本窗口会自动重新检测。',
    // §23：Android 的设置路径完全不同，必须单独一句（不能只换名词）。
    bio_open_settings: '打开 Windows 登录选项',
    bio_manage_hint_android:
      '需要录入、增删指纹时，请到手机的「设置」里打开生物识别 / 指纹页面操作。',
    bio_avail_disabled_by_policy: '{noun} 已被系统策略禁用。',
    bio_avail_busy: '{noun} 暂时被占用，请稍后重试。',
    // D-116：`unsupported` 现在只表示「有适配器、但平台此刻做不了」（如 Windows Hello 的
    // 系统组件无法激活）。「本平台根本没接」已独立成 `not_implemented`，那种情况整卡不渲染，
    // 所以这句不能再写成「当前平台不支持」——那是在为不存在的功能做解释。
    bio_avail_unsupported: '{noun} 组件在当前系统上无法调用，暂时不能使用快速解锁。',
    bio_enable: '开启',
    bio_disable: '关闭',
    bio_enable_hint: '开启前需输入主密码确认身份。',
    bio_empty: '保险库为空时不能开启：没有可校验的密文，快速解锁会永远打不开。请先创建一条记录。',
    bio_disable_confirm: '关闭后将立即删除本机的快速解锁配置，之后只能用主密码解锁。',
    clipboard_ttl: '剪贴板清空（秒）',
    auto_lock: '自动锁定（分钟）',
    auto_lock_updated: '自动锁定档位已更新（{n} 分钟）',
    auto_lock_failed: '读取失败：当前显示的是兜底档位，库里实际档位未能取得。',
    auto_sync: '自动同步',
    sync_on_start: '启动时同步',
    change_password: '修改主密码',
    // D-114：改密成功必须有**留在原地**的明确回执（原先只 toast 一句 `common.ok` =「确定」，
    // 三秒消失，主人实测「右下角一闪而过，看不出到底成没成」）。
    password_changed: '主密码已修改，下次解锁请使用新密码。',
    password_changed_bio_off: '主密码已修改。快速解锁已因此关闭——重新开启后才能继续用 Windows Hello 解锁。',
    password_changed_bio_off_android:
      '主密码已修改。快速解锁已因此关闭——重新开启后才能继续用指纹解锁。',
    change_password_old: '当前主密码',
    change_password_new: '新主密码',
    export_vault: '导出保险库',
    export_confirm_title: '导出保险库',
    // D-54：导出实际是字节级加密副本（unlock_check 后 fs::copy），并非明文；
    // fs::copy 保留 LastWriteTime → 文件修改时间 = 库数据最后写入时刻（客户要求保留该语义）。
    export_confirm_body: '导出的是整库的加密副本（非明文），用于备份或迁移；可在锁屏页用「从备份恢复」导入。文件的修改时间为库数据的最后写入时间。',
    export_password_required: '请先输入当前主密码再导出。',
    vault_file: '保险库文件',
    size: '大小',
    // D-73：标题必须自带解释。主人原话「保险库文件是什么作用？没有看到文件啊？」——
    // 一句「为什么你会关心这个文件」比一个路径有用得多（备份/换机就是复制它）。
    vault_file_hint: '本机全部条目都加密保存在这一个文件里；备份或换机就是复制它，文件名见下。',
    // D-73：读不到时**不能留空**（空 = 静默兜底，把失败伪装成「本来就没有」）。
    vault_file_failed: '读取失败：未能取得库文件路径与大小（条目本身不受影响）。',

    // D-76：保险库压缩入口（体积构成 + 回收动作）。文案语气由主人的原话决定 ——
    // 他问的是「保险库有那么大吗？是不是被删除的附件都还保存在库里？」，
    // 所以「已删附件占 {size}」这一行是**回答**，不是脚注。
    compact: '压缩保险库',
    compact_dead_attachments: '已删附件占 {size}',
    compact_reclaimable: '可回收 {size}',
    compact_none: '没有可回收的空间',
    // 体积构成读不到时**不能留白、也不能说「没有可回收的空间」**——后者是结论，
    // 而我们此刻根本不知道（失败伪装成结论 = D-27 / D-73 同族）。故单独一句。
    compact_stats_failed: '读取失败：未能取得体积构成（条目本身不受影响）。',
    compacting: '正在压缩…',
    compact_wait_sync: '同步进行中，请稍候',
    compact_confirm_title: '压缩保险库？',
    compact_label_before: '当前大小',
    compact_label_after: '压缩后约',
    compact_label_reclaim: '可回收',
    // ⚠️ 规格 §6.11.6 的 15 个键里没有「已删附件占」的**标签形态**（只给了整句
    // compact_dead_attachments）。确认块四行是「左标签 / 右数值」，塞不进整句，
    // 故补这一个标签键；整句那个仍用于卡片正文。已向主理人报备（设计疏漏 #1）。
    compact_label_dead: '已删附件占',
    compact_hint_no_password: '压缩不改任何数据，也不需要输入主密码。',
    compact_hint_trash: '回收站里的附件不会被回收，需先清空回收站。',
    compact_hint_disk: '压缩期间会临时占用约 2 倍磁盘空间，请勿关闭程序。',
    compact_start: '开始压缩',
    compact_done: '已回收 {size}',
    // 压缩失败时的**通用**文案（仅替代 `err.internal`；后端给了具体原因时仍透出原话）。
    // 必须说「原样保留 / 数据没有变化」：压缩是大动作，只说「内部错误」会让用户以为库被写坏。
    compact_failed: '压缩未完成，保险库原样保留，数据没有变化。',
    weak_max_len: '弱口令长度阈值',
    stale_days: '超期天数',
    // D-63：「关于」分组（版本 / 作者 / 许可证 / 项目主页 / 法律条款入口）。
    about_version: '版本',
    about_author: '作者',
    about_license: '许可证',
    about_homepage: '项目主页',
    about_disclaimer: '免责声明',
    about_terms: '用户协议',
    // D-107：更新检查（方案 B：默认关 + 手动按钮）。
    update_auto: '自动检查更新',
    update_auto_hint: '关闭时不发起任何网络请求。开启后每天最多向 GitHub 查询一次最新版本，不含任何用户数据。',
    update_check_now: '检查更新',
    update_checking: '正在检查…',
    update_uptodate: '已是最新版本',
    update_available: '发现新版本 v{version}',
    update_failed: '检查失败，请稍后重试。',
    update_download: '前往下载',
    saved: '设置已保存',
  },
  sync: {
    title: '同步',
    tab_pair: '配对',
    tab_devices: '设备',
    tab_status: '状态',
    tab_conflicts: '冲突',
    start_pairing: '生成配对二维码',
    qr_hint: '请在另一台设备上扫描此二维码完成配对。',
    pair_ip: '本机局域网地址：{ip}',
    firewall_hint: '连不上？请确认手机与本机在同一 Wi‑Fi，并在 Windows 防火墙放行 PasswordMS（允许「专用网络」）。',
    pair_ended: '配对会话已结束。若需再次同步，请重新生成二维码。',
    // D-75：配对**成功**后面板不得立刻收回 —— 手机端此刻才弹出「请核对会话码」，
    // 电脑屏必须把同一个会话码留在屏上，否则核对动作永远没有发生的机会。
    pair_completed: '配对成功',
    pair_completed_hint: '请核对手机上的会话码与本机是否一致，一致才算配对安全。',
    pair_done: '完成',
    manual_hint: '无法扫码？可复制下面的配对文本手动输入。',
    copy_code: '复制配对文本',
    cancel: '取消配对',
    join_title: '加入配对',
    scan_hint: '将二维码置于摄像头前，或粘贴配对文本。',
    camera_unavailable: '无法使用相机：请检查系统是否已授予相机权限，或设备是否有可用摄像头。',
    paste_hint: '粘贴配对文本',
    join: '加入并同步',
    pairing: '等待对端连接…',
    paired: '配对成功',
    fingerprint: '设备指纹',
    devices: '已配对设备',
    no_devices: '还没有已配对设备',
    unpair: '解除配对',
    unpair_confirm: '确定解除与 {name} 的配对？解除后需重新扫码配对。',
    sync_now: '立即同步',
    syncing: '正在同步…',
    last_sync: '上次同步',
    pending: '待同步变更',
    device_count: '设备数',
    result_applied: '已合并 {n} 条',
    result_conflicts: '产生 {n} 个冲突',
    adopt_requires_empty: '本机保险库非空（{n} 条），无法直接采用对端密钥。若两台设备此前已配对，请改用「设备 → 立即同步」；若各自是独立保险库，则无法直接合并，请先导出并清空一方，或改用对端重新创建保险库。',
    never: '从未',
    // D-66：设备可辨识。「重命名 / 保存 / 取消」复用既有 common.* 键（不造重复键）；
    // 这里只补设备列表独有的「提示语 / 成功文案 / 配对时间」。
    rename_hint: '给这台设备起个好认的名字',
    renamed: '设备名称已更新',
    paired_at: '配对时间',
    // 后端 `Repository::rename_paired_device` 的上限是 64 **字节**（22 个汉字即超限），
    // 与 `maxlength`（UTF-16 码元）不同口径，故需单独提示。
    rename_too_long: '名称过长：最多 64 字节（约 21 个汉字）',
    // D-69：配对两屏核对的是**会话码**（取代 D-68 的指纹核对 —— 原来两屏算的不是同一个量：
    // 电脑屏是长期身份指纹、手机屏是本次临时公钥指纹，必然不等，用户每次都会看到「不一致」）。
    // 方向固定：电脑屏显本次会话码，手机屏显「对端（= 电脑）」的同一个值。
    // ⚠️ 这三个键的文案里不得出现「指纹」字样，否则用户会以为要核对的是设备指纹。
    session_code: '本次配对核对码',
    peer_session_code: '电脑上的核对码',
    session_code_hint: '请与另一台设备上显示的核对码逐字核对；不一致说明本次连接可能被中间人隔开——请立即解除配对并重试。',
  },
  conflict: {
    title: '冲突仲裁',
    none: '没有待处理冲突',
    local: '本机版本',
    remote: '远端版本',
    keep_local: '保留本机',
    take_remote: '采用远端',
    keep_both: '两者都留',
    choose: '请选择处理方式',
    local_title: '本机标题',
    remote_title: '远端标题',
    detected_at: '发现于',
    // D-112：KeepBoth 副本标题后缀（保留前导空格）。
    copy_suffix: ' (冲突副本)',
  },
  // D-63：法律条款弹窗（免责声明 / 用户协议）的标题与按钮文案；
  // 正文不从 i18n 取，见 src/lib/legal-content.ts（逐条实质内容 + 统一 updated）。
  legal: {
    disclaimer_title: '免责声明',
    terms_title: '用户协议',
    understood: '我知道了',
    updated: '最后更新',
  },
  err: {
    startup_failed: '启动状态查询失败，请重试。',
    clipboard_clear_failed: '剪贴板自动清空失败，请手动清理。',
    unlock_failed: '解锁失败，请重试。',
    wrong_master_password: '主密码错误，请重新输入。',
    no_vault: '未找到保险库。',
    vault_exists: '保险库已存在。',
    weak_password: '主密码强度不足。',
    not_unlocked: '保险库处于锁定状态。',
    item_not_found: '条目不存在。',
    folder_not_found: '分组不存在。',
    folder_name: '分组名称无效。',
    validation: '输入无效。',
    // D-97：TOTP 密钥入库前校验失败（编辑器）与展示错误态（详情页）。
    // 必须点名合法形态（Base32 / otpauth 链接）——只说「输入无效」用户不知道改什么。
    totp_invalid:
      '动态口令密钥无效：须为 Base32 密钥（字母 A-Z、数字 2-7，不含 0/1/8/9），或直接粘贴 otpauth:// 链接自动提取。',
    totp_invalid_detail: '动态口令密钥无效，请编辑条目修正。',
    attachment_too_large: '附件超过大小上限。',
    attachment_limit: '附件数量已达上限（16）。',
    attachment_not_found: '附件不存在（可能已被删除）。',
    vault_capacity: '保险库容量已满。',
    unsafe_url: '该网址不安全，已阻止打开。',
    import_parse: '导入文件解析失败：请确认是未加密的 Bitwarden 导出（JSON / CSV）且内容完整。',
    import_encrypted: '不支持加密导出文件。',
    pair_token_invalid: '配对信息无效。',
    pair_token_expired: '配对二维码已过期，请重新生成。',
    pair_token_used: '该配对信息已被使用。',
    not_paired: '设备尚未配对。',
    replay: '检测到重放攻击，已中止同步。',
    fingerprint: '设备指纹校验失败，已中止同步。',
    sync_aborted: '同步已中止。',
    sync_busy: '已有同步在进行中，请稍后再试。',
    // D-108（豆包审查 3.5-10）：解锁后五路加载失败的合并提示（重试一次后仍失败才弹）。
    load_partial: '部分数据加载失败，请重试或重新锁定后再试。',
    conflict: '存在未处理的冲突。',
    vk_adopt_requires_empty: '本机保险库非空，无法直接采用对端密钥。',
    restore_conflict: '无法还原：该条目已存在新版本（多设备同步所致），请在条目列表或冲突箱中处理。',
    blob_missing: '附件内容尚未同步到本机，请在已同步后重试。',
    blob_incomplete: '附件内容传输不完整，请重新同步。',
    blob_capacity: '附件超出本机保险库剩余容量。',
    // D-113 P2：快速解锁的失败分类（`docs/08` §10）——**不得**合并成一句「解锁失败」，
    // 否则用户无法判断是「取消了一次」还是「该重新开启」。
    biometric_unavailable: '本机当前无法使用快速解锁，请用主密码解锁。',
    biometric_canceled: '已取消 Windows Hello 验证。',
    biometric_canceled_android: '已取消指纹验证。',
    biometric_invalidated: '快速解锁已失效，请用主密码解锁后在设置中重新开启。',
    need_master_password: '这次解锁无法校验保险库密钥，请输入主密码解锁。',
    bio_invalidated_password: '主密码已修改，快速解锁已失效，请重新开启。',
    bio_invalidated_vault_key: '保险库密钥已更换（配对加入），快速解锁已失效，请重新开启。',
    bio_invalidated_enrollment: 'Windows Hello 配置已变更，快速解锁已失效，请重新开启。',
    bio_invalidated_enrollment_android: '设备的生物识别记录已变更，快速解锁已失效，请重新开启。',
    bio_invalidated_tampered: '快速解锁配置被篡改或损坏，已失效，请重新开启。',
    io: '文件读写出错。',
    network_timeout: '网络响应超时：请确认两台设备在同一局域网、对方仍停在配对等待页，然后重试。',
    network_refused: '连接被拒绝：对方可能已离开配对页，或防火墙拦下了端口，请对方重新展示二维码。',
    network_broken: '连接中途断开：请在两台设备上重新发起配对。',
    // v1.2.4 根因 C：原文「无法连接到电脑…请确认手机与电脑…」写死了方向，
    // 电脑端触发同一条错误时读起来完全反了。改为**方向中性**：两端都用同一条文案。
    network_unreachable: '无法连接到对端设备：请确认两台设备连的是同一个 Wi‑Fi（同一网段）；Windows 防火墙需放行 PasswordMS（专用网络）。若刚刚在另一台设备上同步成功过，请再试一次；仍失败请重新配对。',
    crypto: '加密操作失败。',
    internal: '发生内部错误。',
    unknown: '发生未知错误。',
  },
} as const;

/** 深度字符串化：把字面量类型放宽为 `string`，用于约束其他语言键结构一致。 */
type DeepString<T> = {
  [K in keyof T]: T[K] extends string ? string : DeepString<T[K]>;
};

/** 文案字典类型（由中文推导，保证 en-US 键完整）。 */
export type Messages = DeepString<typeof zhCN>;
