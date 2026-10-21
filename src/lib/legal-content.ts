// 法律文本数据源（免责声明 / 用户协议，D-63）。
//
// 纯数据模块：无副作用、不触碰任何 window/tauri 全局，供 `LegalDialog.svelte` 渲染，
// 由 `legal-content.test.ts` 守门（双语齐全 / 条数固定 / 每条有实质内容 / updated 一致）。
//
// 文本口径（主人 2026-10-21 需求批）：
// - 免责声明 6 条：主密码无找回、按现状提供、不构成专业安全建议、局域网同步风险、
//   数据丢失风险、无担保与责任限制；
// - 用户协议 8 条：MIT 许可授权、使用限制、数据与隐私、开源组件、协议变更、终止、
//   适用法律（中华人民共和国法律）、生效日期；
// - 两份文档 updated 统一 `2026-10-20`（与 v1.2.0 公开发布一致）。

/** 法律文本语言（与 `ui.locale` 的映射见 `LegalDialog.svelte`：zh-CN → 'zh'，en-US → 'en'）。 */
export type LegalLang = 'zh' | 'en';

/** 单条条款：条目标题 + 若干段落实质内容。 */
export interface LegalSection {
  heading: string;
  paragraphs: string[];
}

/** 一份法律文档。 */
export interface LegalDoc {
  title: string;
  /** 文本最后更新日期（ISO `YYYY-MM-DD`）。 */
  updated: string;
  sections: LegalSection[];
}

/** 两份文档统一的更新日期（含生效日，见用户协议第 8 条）。 */
export const LEGAL_UPDATED = '2026-10-20';

const DISCLAIMER_ZH: LegalDoc = {
  title: '免责声明',
  updated: LEGAL_UPDATED,
  sections: [
    {
      heading: '主密码不可找回',
      paragraphs: [
        'PasswordMS 为本地优先的零知识密码管理器：主密码仅在你本机用于派生加密密钥，软件不设任何后门，也不提供密钥托管、找回或重置服务。',
        '一旦忘记主密码，保险库中的数据将永久无法解密与恢复，作者无法也不应为你绕过加密。请务必定期使用「设置 → 数据 → 导出保险库」生成加密备份，并妥善保管备份文件与主密码。',
      ],
    },
    {
      heading: '软件按「现状」提供',
      paragraphs: [
        '本软件按「现状」（AS IS）提供，不附带任何形式的明示或默示保证，包括但不限于适销性、特定用途适用性与无侵权保证。',
        '作者不保证软件完全无缺陷、不中断或与你的所有设备、系统版本完全兼容；使用本软件的风险由你自行承担。',
      ],
    },
    {
      heading: '不构成专业安全建议',
      paragraphs: [
        '本软件及随附文档不构成任何形式的法律、合规或专业安全建议。你应结合自身场景（如企业合规要求）另行评估。',
        '请在可信设备上使用本软件，及时安装操作系统与应用更新（含 WebView2 运行时更新），并为设备启用屏幕锁定与磁盘加密等基础防护。',
      ],
    },
    {
      heading: '局域网同步风险',
      paragraphs: [
        '跨设备同步仅在你主动配对的两台设备之间、通过局域网点对点完成，同步数据全程为端到端加密的密文。',
        '请避免在不受信任的网络（如公共 Wi-Fi、机场与酒店开放网络）中执行配对；配对时请核对设备指纹，防止与伪造设备建立连接。',
      ],
    },
    {
      heading: '数据丢失风险',
      paragraphs: [
        '设备损坏、系统重装、误删除、主密码遗忘或同步误操作等均可能导致数据丢失。请保持定期导出备份的习惯。',
        '因上述情形造成的数据丢失或损坏，作者不承担责任；软件内提供的回收站与备份恢复功能仅为降低风险，不构成数据安全的兜底承诺。',
      ],
    },
    {
      heading: '无担保与责任限制',
      paragraphs: [
        '在法律允许的最大范围内，作者不对任何间接损失、附带损失、特殊损失、惩罚性损失或利润损失、业务中断损失承担责任。',
        '无论基于合同、侵权或其他任何理论，作者对你承担的全部责任总额不超过你为获得本软件实际支付的费用（本软件为免费开源软件，该金额为零）。',
      ],
    },
  ],
};

const DISCLAIMER_EN: LegalDoc = {
  title: 'Disclaimer',
  updated: LEGAL_UPDATED,
  sections: [
    {
      heading: 'Master password cannot be recovered',
      paragraphs: [
        'PasswordMS is a local-first, zero-knowledge password manager: your master password is used only on your own device to derive encryption keys. The software contains no backdoors and offers no key escrow, recovery, or reset service.',
        'If you forget your master password, the vault contents become permanently undecryptable and unrecoverable. The author cannot and will not bypass the encryption for you. Export an encrypted backup regularly via Settings → Data → Export vault, and keep both the backup file and your master password safe.',
      ],
    },
    {
      heading: 'Provided "as is"',
      paragraphs: [
        'This software is provided "AS IS", without warranty of any kind, express or implied, including but not limited to the warranties of merchantability, fitness for a particular purpose, and non-infringement.',
        'The author does not warrant that the software is free of defects, uninterrupted, or fully compatible with every device and system version you use. You use the software at your own risk.',
      ],
    },
    {
      heading: 'Not professional security advice',
      paragraphs: [
        'The software and its documentation do not constitute legal, compliance, or professional security advice of any kind. Evaluate it against your own requirements (for example, corporate compliance obligations).',
        'Use the software on trusted devices, keep your operating system and application runtimes (including the WebView2 runtime) up to date, and enable screen lock and disk encryption as basic protections.',
      ],
    },
    {
      heading: 'LAN sync risks',
      paragraphs: [
        'Cross-device sync happens only between the two devices you explicitly paired, peer-to-peer over your local network, and all synced data stays end-to-end encrypted.',
        'Avoid pairing on untrusted networks (public Wi-Fi, airports, hotels). Always verify the device fingerprint during pairing to avoid connecting to a spoofed device.',
      ],
    },
    {
      heading: 'Risk of data loss',
      paragraphs: [
        'Device failure, OS reinstallation, accidental deletion, a forgotten master password, or a mistaken sync operation can all cause data loss. Keep the habit of exporting regular backups.',
        'The author is not liable for data loss or corruption caused by the situations above. The built-in trash and backup-restore features reduce risk; they are not a guarantee of data safety.',
      ],
    },
    {
      heading: 'No warranty and limitation of liability',
      paragraphs: [
        'To the maximum extent permitted by law, the author shall not be liable for any indirect, incidental, special, punitive, or consequential damages, or for loss of profits or business interruption.',
        'Whether based on contract, tort, or any other theory, the author\u2019s total aggregate liability to you shall not exceed the amount you actually paid to obtain this software (this is free open-source software, so that amount is zero).',
      ],
    },
  ],
};

const TERMS_ZH: LegalDoc = {
  title: '用户协议',
  updated: LEGAL_UPDATED,
  sections: [
    {
      heading: '许可授权',
      paragraphs: [
        '本软件依据 MIT 许可证授权：你可以自由使用、复制、修改、合并、发布、分发、再许可和/或销售本软件的副本。',
        '上述授权的前提是：在所有副本或重要部分中保留原始版权声明与许可声明。完整条款以仓库中的 LICENSE 文件为准。',
      ],
    },
    {
      heading: '使用限制',
      paragraphs: [
        '你不得将本软件用于任何违法用途，或用于侵害他人合法权益（包括但不限于未经授权访问他人数据）。',
        '你不得以任何方式暗示作者为你的产品、服务或行为提供背书；修改后的分发版本应清晰标注修改内容，不得冒充官方版本。',
      ],
    },
    {
      heading: '数据与隐私',
      paragraphs: [
        '你的全部数据仅保存在本机保险库文件以及你主动配对的设备上；软件不联网上传任何数据，作者无法访问你的保险库内容。',
        '软件不收集遥测、崩溃日志或使用统计。你与作者之间不存在任何数据受托关系；数据的备份与保管责任由你自行承担。',
      ],
    },
    {
      heading: '开源组件',
      paragraphs: [
        '本软件使用了 Tauri、Svelte、Rust 生态等开源组件，这些组件分别遵循其各自的许可证，相关声明见各依赖仓库。',
        '本协议不影响、也不替代任何第三方组件的许可证条款。',
      ],
    },
    {
      heading: '协议变更',
      paragraphs: [
        '本协议可能随新版本更新。条款更新随对应版本发布即生效，作者将在版本说明中注明重大变更。',
        '若条款与仓库中的 LICENSE、README 存在冲突，以仓库中的文件为准。',
      ],
    },
    {
      heading: '终止',
      paragraphs: [
        '你可以随时停止使用本软件，并删除本机上的程序与保险库文件以终止本协议。',
        '终止后，关于免责、责任限制与许可声明的条款继续有效。',
      ],
    },
    {
      heading: '适用法律',
      paragraphs: [
        '本协议的订立、效力、解释与争议解决均适用中华人民共和国法律。',
        '因本协议产生的争议，双方应友好协商解决；协商不成的，提交作者所在地有管辖权的人民法院处理。',
      ],
    },
    {
      heading: '生效日期',
      paragraphs: ['本协议自 2026-10-20 起生效，与 PasswordMS v1.2.0 公开发布一致。'],
    },
  ],
};

const TERMS_EN: LegalDoc = {
  title: 'Terms of Use',
  updated: LEGAL_UPDATED,
  sections: [
    {
      heading: 'License grant',
      paragraphs: [
        'This software is licensed under the MIT License: you may freely use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the software.',
        'The grant is conditioned on retaining the original copyright notice and permission notice in all copies or substantial portions of the software. The LICENSE file in the repository is authoritative.',
      ],
    },
    {
      heading: 'Acceptable use',
      paragraphs: [
        'You may not use this software for any unlawful purpose or to infringe the lawful rights of others (including unauthorized access to another person\u2019s data).',
        'You may not imply that the author endorses your product, service, or conduct. Modified distributions must clearly state the changes and must not impersonate the official release.',
      ],
    },
    {
      heading: 'Data and privacy',
      paragraphs: [
        'All of your data lives only in the local vault file and on the devices you explicitly paired. The software uploads nothing, and the author cannot access your vault contents.',
        'The software collects no telemetry, crash logs, or usage statistics. There is no data-fiduciary relationship between you and the author; backing up and safeguarding your data is your responsibility.',
      ],
    },
    {
      heading: 'Open-source components',
      paragraphs: [
        'This software uses open-source components such as Tauri, Svelte, and the Rust ecosystem. Each component remains under its own license as declared in its repository.',
        'These terms neither modify nor replace the license terms of any third-party component.',
      ],
    },
    {
      heading: 'Changes to these terms',
      paragraphs: [
        'These terms may be updated with new releases. Updated terms take effect when the corresponding version is published, and material changes will be noted in the release notes.',
        'If these terms conflict with the LICENSE or README in the repository, the repository files prevail.',
      ],
    },
    {
      heading: 'Termination',
      paragraphs: [
        'You may stop using the software at any time and delete the program and vault files from your device to terminate these terms.',
        'The provisions on disclaimers, limitation of liability, and licensing survive termination.',
      ],
    },
    {
      heading: 'Governing law',
      paragraphs: [
        'The formation, validity, interpretation, and dispute resolution of these terms are governed by the laws of the People\u2019s Republic of China.',
        'Disputes arising from these terms should first be resolved through friendly negotiation; failing that, they shall be submitted to a competent people\u2019s court at the author\u2019s domicile.',
      ],
    },
    {
      heading: 'Effective date',
      paragraphs: ['These terms take effect on 2026-10-20, aligning with the public release of PasswordMS v1.2.0.'],
    },
  ],
};

/** 取免责声明（按语言）。 */
export function getDisclaimer(lang: LegalLang): LegalDoc {
  return lang === 'en' ? DISCLAIMER_EN : DISCLAIMER_ZH;
}

/** 取用户协议（按语言）。 */
export function getTerms(lang: LegalLang): LegalDoc {
  return lang === 'en' ? TERMS_EN : TERMS_ZH;
}
