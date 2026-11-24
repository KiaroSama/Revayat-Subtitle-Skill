<div dir="rtl">

<div align="center">

# ‏⁧روایت زیرنویس (Revayat Subtitle)⁩

‏⁧**زیرنویس‌ها را با خروجی پیش‌فرض ASS (یا SRT به انتخاب شما) ترجمه، ویرایش و با صدای ویدئو همگام‌سازی کنید و یک بستهٔ بازبینی‌شده در `Sub.zip` بگیرید. زبان پیش‌فرض خروجی فارسی است.**⁩

[![GitHub Actions CI](https://github.com/KiaroSama/Revayat-Subtitle-Skill/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/KiaroSama/Revayat-Subtitle-Skill/actions/workflows/ci.yml)
[![GNU General Public License v3.0 or later](https://img.shields.io/badge/license-GPL--3.0--or--later-2563eb)](LICENSE)
[![Latest GitHub tag](https://img.shields.io/github/v/tag/KiaroSama/Revayat-Subtitle-Skill?sort=semver&include_prereleases&label=tag&color=ea580c)](https://github.com/KiaroSama/Revayat-Subtitle-Skill/tags)
[![Automated pytest and cargo test suites across CI matrices](https://img.shields.io/badge/tests-pytest%20%7C%20cargo-0d9488)](.github/workflows/ci.yml)
[![Python 3.13.15 measured locally, requires Python 3.11 or newer](https://img.shields.io/badge/Python-3.13.15-8b5cf6)](docs/platforms.md)
[![Rust 1.100.0 measured locally for audio sync and PyO3 acceleration crates](https://img.shields.io/badge/Rust-1.100.0-9a3412)](rust/subsync-rs/Cargo.toml)
[![Runs on Windows, Linux, and macOS](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-0891b2)](docs/platforms.md)
[![Install via five plugin formats or standalone scripts](https://img.shields.io/badge/install-5%20plugins%20%7C%20standalone-4f46e5)](#نصب)
[![Documentation in English and Persian](https://img.shields.io/badge/docs-English%20%7C%20Persian-0284c7)](README.md)
[![Published Git tree size measured via git ls-files is 1.71 MB](https://img.shields.io/badge/repo%20size-1.71%20MB-ca8a04)](#repository-size)
[![ASS default output with bundled B Koodak font and Translator Mode](https://img.shields.io/badge/output-ASS%20%7C%20B%20Koodak-15803d)](skills/revayat-subtitle/references/subtitle-formats.md)
[![Rust FFT and ALASS audio-to-subtitle synchronization engine](https://img.shields.io/badge/audio%20sync-Rust%20FFT%20%7C%20ALASS-b45309)](skills/revayat-subtitle/references/audio-sync.md)
[![FFmpeg and libass visual verification pipeline](https://img.shields.io/badge/renderer-FFmpeg%20%7C%20libass-334155)](skills/revayat-subtitle/references/persian-typography.md)
[![Three bundled voice guides for Persian, English, and Russian](https://img.shields.io/badge/voice%20guides-3%20bundled-7c3aed)](skills/revayat-subtitle/references/target-voice.md)
[![CodeQL static analysis and Gitleaks secret scanning](https://img.shields.io/badge/security-CodeQL%20%7C%20Gitleaks-c2410c)](.github/workflows/codeql.yml)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

</div>

<a id="repository-size"></a>

‏⁧حجم درخت منتشرشده در Git (نسخهٔ `2.0.0`، اندازه‌گیری‌شده با `git ls-files`): ۱٬۷۱۱٬۴۵۲ بایت (۱٫۷۱ مگابایت) در ۲۱۹ فایل ردیابی‌شده، بدون تاریخچهٔ Git و فایل‌های ثبت‌نشده.⁩

‏⁧اسکیلی برای Claude Code، Codex، Cursor، Antigravity، Kiro، Cline، Hermes، OpenCode و هر ایجنتی که `SKILL.md` را می‌خواند. این ابزار بخش‌هایی را حل می‌کند که در ترجمهٔ ماشینی خراب می‌شوند: ردیابی تمام خطوط در چند نسخه، یکدستی نام‌ها و پسوندهای ژاپنی بین فصل‌ها، حفظ افکت‌های ASS، خروجی پیش‌فرض ASS با فونت همراه `B Koodak`، مهار پرش علائم نگارشی در خطوط دوسطری فارسی، ساخت فایل حالت مترجم و همگام‌سازی مستقیم زمان‌بندی زیرنویس با صدای ویدئو به زبان Rust.⁩

<div align="right"><a href="LICENSE">‏⁧مجوز GPL-3.0⁩</a></div>
<div align="left"><a href="README.md">English</a></div>

---

## ‏⁧قابلیت‌های کلیدی⁩

| ‏⁧قابلیت⁩ | ‏⁧نحوهٔ عملکرد⁩ |
| --- | --- |
| ‏⁧خروجی پیش‌فرض ASS با فونت `B Koodak`⁩ | ‏⁧خروجی استاندارد 1080p ASS (`PlayResX: 1920`، `PlayResY: 1080`) با فونت همراه `BKOODB.TTF` (`B Koodak` Bold)، حاشیهٔ مشکی ظریف (`1.5`) و سایهٔ ملایم (`0.8`) تولید می‌شود و از طریق پرچم‌های CLI قابل تغییر است. قالب SRT با `--output-format srt` همچنان در دسترس است.⁩ |
| ‏⁧مهار پرش علائم نگارشی و شکست متعادل خط⁩ | ‏⁧خطوط طولانی به شکل متعادل در حداکثر ۲ خط (`\N` با سقف ۴۲ کاراکتر) بدون شکستن تگ‌های ASS یا کلمات نیم‌فاصله‌دار تقسیم می‌شوند و هر خط نمایشی با `RLE` (`U+202B`)، `RLM` پایانی (`U+200F`) و `PDF` (`U+202C`) قفل می‌شود تا نقطه، علامت سؤال و گیومه در VLC، mpv، PotPlayer و libass به خط اول نپرند.⁩ |
| ‏⁧نیم‌فاصلهٔ فارسی و حذف خودکار واترمارک⁩ | ‏⁧نیم‌فاصله را در «می‌»/«نمی‌»، «‌ها»، «‌تر»/«‌ترین» و پسوندهای «ه‌» اعمال می‌کند، حروف عربی `ي`/`ك`/`ة`/`ى` را بیرون از تگ‌ها و پیوندها به فارسی برمی‌گرداند و تبلیغات مترجم، استودیو، کانال تلگرام و نشانی وب را با گزارش ساختاریافته حذف می‌کند.⁩ |
| ‏⁧حفظ کج‌نویسی، ضخیم‌نویسی و افکت‌ها⁩ | ‏⁧تگ‌های `<i>`، `<b>`، `<u>`، `<s>`، `<font color>` و `{an1..9}` را در SRT به معادل ASS (`{\i1}`، `{\b1}`، `{\c&HBBGGRR&}`، `{\an8}`) تبدیل می‌کند و مسیرهای برداری، کارائوکه و لایه‌های موجود ASS را دست‌نخورده نگه می‌دارد.⁩ |
| ‏⁧حالت مترجم (`translator-ass`)⁩ | ‏⁧نسخهٔ اصلی و نسخه‌های کمکی را به ترتیب زمانی ترکیب می‌کند، برای خطوط بدون هم‌پوشانی در نسخه‌های کمکی ردیف‌های زمان‌بندی‌شدهٔ خالی (`Dialogue:`) می‌سازد و در صورت نیاز متن منبع را به‌صورت `Comment:` درج می‌کند.⁩ |
| ‏⁧همگام‌سازی صوتی Rust (`subsync-rs`)⁩ | ‏⁧صدای مونو ۱۶ کیلوهرتز را با FFmpeg استخراج می‌کند، با تشخیص فعالیت گفتار دوویژگی (FFT) و همبستگی متقابل `rustfft` جابه‌جایی سراسری و رانش فریم‌ریت PAL/NTSC را می‌یابد، برش‌های میانی را با برنامه‌ریزی پویا (ALASS) پیدا می‌کند و لبه‌های خط را به مرز گفتار می‌چسباند. به‌صورت اسکیل هم‌تراز `revayat-subtitle-sync` و دستور `revayat-subtitle.py sync` در دسترس است.⁩ |
| ‏⁧شتاب‌دهنده‌های Rust PyO3⁩ | ‏⁧بخش‌های سنگین پردازش متن، بازبینی فارسی و نشانه‌گذاری با ابزار `rust-anything` به Rust خالص (`rust/subcore-rs`، `rust/persian-lint-rs`، `rust/markup-rs`) با `#![forbid(unsafe_code)]` و بازگشت امن به پایتون (`REVAYAT_DISABLE_RUST=1`) پورت شده‌اند.⁩ |
| ‏⁧ردیابی ویرایشی چندنسخه‌ای⁩ | ‏⁧شناسهٔ پایدار خطوط، هش منبع، واژه‌نامهٔ قفل‌شده و برگه‌های کامل جلوی جاماندن خطوط و تغییر نام شخصیت‌ها در فصل‌های بعد را می‌گیرند.⁩ |
| ‏⁧بازبینی بصری با FFmpeg و libass⁩ | ‏⁧فریم‌های PNG را با پوشهٔ فونت همراه (`:fontsdir=fonts`) رندر می‌کند تا ایجنت پیش از `qa` و بسته‌بندی `Sub.zip` ظاهر واقعی حروف، تعادل خط و جهت فارسی/انگلیسی را ببیند.⁩ |

## ‏⁧نصب⁩

‏⁧به **`Python 3.11` یا جدیدتر** و FFmpeg با پشتیبانی libass نیاز است. اسکریپت‌های پایتون فقط از کتابخانهٔ استاندارد استفاده می‌کنند و فونت `B Koodak` Bold همراه مخزن در `skills/revayat-subtitle/assets/fonts/BKOODB.TTF` قرار دارد ([اطلاعیهٔ فونت](skills/revayat-subtitle/assets/fonts/NOTICE.txt)). فقط برای ساخت محلی موتور همگام‌سازی صدا یا افزونه‌های اختیاری PyO3 به Rust `1.100.0` نیاز دارید.⁩

<div dir="ltr">

```bash
git clone https://github.com/KiaroSama/Revayat-Subtitle-Skill.git
cd Revayat-Subtitle-Skill
python -m pip install -r skills/revayat-subtitle/requirements.txt
```

</div>

‏⁧سپس هر دو اسکیل `revayat-subtitle` و `revayat-subtitle-sync` را در ایجنت‌ها نصب کنید:⁩

<div dir="ltr">

```bash
# macOS / Linux
bash install/install.sh
```

```powershell
# Windows
./install/install.ps1
```

</div>

‏⁧نصب‌کننده به‌طور پیش‌فرض پوشه‌های کاربری موجود را انتخاب می‌کند. برای یک ایجنت خاص از `--agent codex`، برای نصب پروژه‌ای از `--scope project --path PATH`، برای پیش‌نمایش از `--dry-run` و برای جایگزینی با پشتیبان از `--force` استفاده کنید. جزئیات کامل مسیرها در [docs/platforms.md](docs/platforms.md) آمده است.⁩

<div dir="ltr">

| Agent | User skills | Project skills |
| --- | --- | --- |
| Claude Code | `~/.claude/skills/` | `.claude/skills/` |
| Codex | `~/.agents/skills/` | `.agents/skills/` |
| Cursor | `~/.cursor/skills/` | `.cursor/skills/` |
| Kiro | `~/.kiro/skills/` | `.kiro/skills/` |
| Cline | `~/.cline/skills/` | `.cline/skills/` |
| Hermes | `~/.hermes/skills/` | `.hermes/skills/` |
| OpenCode | `~/.config/opencode/skills/` | `.opencode/skills/` |
| Antigravity IDE | `~/.gemini/antigravity/skills/` | `.agents/skills/` |
| Antigravity CLI (`antigravity-cli`) | `~/.gemini/antigravity-cli/skills/` | `.agents/skills/` |

</div>

### ‏⁧نصب به‌صورت پلاگین (۵ قالب استاندارد)⁩

‏⁧مخزن هر ۵ قالب مانیفست پلاگین نسخهٔ `2.0.0` را روی یک درخت اسکیل مشترک عرضه می‌کند:⁩

- ‏⁧**Claude Code:** `.claude-plugin/plugin.json` و `.claude-plugin/marketplace.json`⁩
- ‏⁧**Cursor:** `.cursor-plugin/plugin.json` و `.cursor-plugin/marketplace.json`⁩
- ‏⁧**Codex:** `.codex-plugin/plugin.json` و `.agents/plugins/marketplace.json`⁩
- ‏⁧**Antigravity:** `.antigravity-plugin/plugin.json`⁩
- ‏⁧**Agent Plugins 1.0:** فایل ریشهٔ `plugin.json`⁩

‏⁧نصب در Claude Code:⁩

<div dir="ltr">

```text
/plugin marketplace add KiaroSama/Revayat-Subtitle-Skill
/plugin install revayat-subtitle@revayat-subtitle-marketplace
```

</div>

‏⁧پلاگین فرمان‌های `translate-subtitles`، `revayat-subtitle-resume`، `revayat-subtitle-qa` و `revayat-subtitle-sync` را در فضای نام `revayat-subtitle` ارائه می‌دهد. ساخت بستهٔ محلی پلاگین:⁩

<div dir="ltr">

```bash
python install/install.py --plugin --destination "PATH/revayat-subtitle" --recovery-dir "PATH/recovery"
```

</div>

### ‏⁧ساخت باینری همگام‌سازی صوتی Rust (اختیاری)⁩

<div dir="ltr">

```bash
cargo build --release --manifest-path rust/subsync-rs/Cargo.toml
```

</div>

### ‏⁧بررسی نصب⁩

<div dir="ltr">

```bash
python skills/revayat-subtitle/scripts/revayat-subtitle.py doctor
```

</div>

‏⁧خروجی `ready: true` در دسترس‌بودن FFmpeg و libass را تأیید می‌کند. اگر FFmpeg در `PATH` نیست، از `--ffmpeg PATH` یا متغیر `REVAYAT_FFMPEG` استفاده کنید.⁩

## ‏⁧استفاده⁩

‏⁧از ایجنت بخواهید زیرنویس‌ها را ترجمه، تبدیل، به حالت مترجم آماده یا با ویدئو همگام کند:⁩

> ‏⁧از revayat-subtitle برای این زیرنویس‌ها استفاده کن. نام شخصیت‌ها و پسوندهای ژاپنی را یکدست نگه دار، فارسی گفتاری با خروجی ASS و فونت B Koodak بساز، واترمارک‌ها را حذف کن و فایل نهایی را با episode01.mkv همگام‌سازی کن.⁩

‏⁧زبان پیش‌فرض خروجی فارسی است. برای روسی یا انگلیسی، [راهنمای لحن زبان مقصد](skills/revayat-subtitle/references/target-voice.md) به‌کار گرفته می‌شود.⁩

### ‏⁧اجرای مستقیم از خط فرمان⁩

<div dir="ltr">

```bash
PY=python3
S=skills/revayat-subtitle/scripts

# 1. Environment check
"$PY" "$S/revayat-subtitle.py" doctor

# 2. Convert SRT to default Persian ASS (B Koodak, outline 1.5, shadow 0.8, ZWNJ, RTL wrap, watermark strip)
"$PY" "$S/revayat-subtitle.py" convert episode.srt --out episode.ass

# 3. Build a Translator Mode ASS from a primary release plus secondary donors
"$PY" "$S/revayat-subtitle.py" translator-ass french.srt --donor dutch.srt --out translator.ass

# 4. Audio-sync a subtitle directly to a video using Rust subsync-rs
"$PY" "$S/revayat-subtitle.py" sync episode.mkv episode.ass --out episode.synced.ass

# 5. Full season editorial workflow
"$PY" "$S/revayat-subtitle.py" prepare season.zip --work work/season01 --series "Anime title" --season 1
"$PY" "$S/revayat-subtitle.py" build --work work/season01
"$PY" "$S/revayat-subtitle.py" render --build "$BUILD" --episode S01E01 --video episode01.mkv
"$PY" "$S/revayat-subtitle.py" qa --build "$BUILD"
"$PY" "$S/revayat-subtitle.py" package --build "$BUILD" --out out/Sub.zip
```

</div>

‏⁧در PowerShell، مقدارهای `$PY = 'python'` و `$S = 'skills/revayat-subtitle/scripts'` را تنظیم و دستورها را با `& $PY` اجرا کنید.⁩

‏⁧**ورودی‌ها:** ASS، SRT، پوشه و ZIP (و فایل‌های ویدئویی MKV/MP4/WAV برای `sync`). **خروجی:** به‌طور پیش‌فرض ASS (`S01E01.ass`، `S01OVA01.ass`) و در صورت درخواست SRT (`--output-format srt`)، بسته‌بندی‌شده در `Sub.zip` کنار واژه‌نامهٔ به‌روزشده.⁩

### ‏⁧ادامهٔ کار و پیشنهادهای فقط‌خواندنی⁩

‏⁧فرمان `status --work WORK` خطوط باقی‌مانده، وضعیت واژه‌نامه و شواهد نسخهٔ فعال را گزارش می‌کند و `handoff --work WORK --assignment FILE --result FILE` پیشنهادهای بدون تداخل ساب‌ایجنت‌ها را بدون واردکردن خودکار راستی‌آزمایی می‌کند. جزئیات کامل در [workflow.md](skills/revayat-subtitle/references/workflow.md#read-only-status-and-editorial-handoffs)، [روبریک](evaluation/rubric.md) و [پایلوت](evaluation/pilot.json) آمده است.⁩

## ‏⁧معماری⁩

<div dir="ltr">

```text
ASS / SRT / ZIP (+ optional MKV / MP4 / WAV)
       |
       v
prepare -> immutable sources + project.json + complete worksheets
       |                             |
       |                     research + locked glossary
       v                             |
compare releases <------------------+
       |
       v
agent translates and records every cue decision (ZWNJ, watermark filter, markup)
       |
       v
build -> ASS default (B Koodak, outline/shadow, balanced RTL wrap) or requested SRT
       |
       v
optional sync -> Rust subsync-rs (16 kHz FFT VAD + cross-correlation + ALASS DP)
       |
       v
render -> FFmpeg + libass PNGs (bundled fontsdir) -> agent visual inspection
       |
       v
qa -> source, edition, glossary, and image evidence must agree
       |
       v
package -> Sub.zip, read back and verified against the reviewed build
```

</div>

## ‏⁧آنچه صادقانه باید گفت⁩

- ‏⁧اسکریپت‌ها ساختار، هش‌ها، قواعد نیم‌فاصله، نشانه‌های RTL، زمان‌بندی و شواهد رندر را تضمین می‌کنند؛ دقت معنایی و طبیعی‌بودن ترجمه همچنان به این وابسته است که مدل تمام دیالوگ‌ها را در بافتشان بخواند.⁩
- ‏⁧همگام‌سازی صوتی (`subsync-rs`) خطوط را با فعالیت گفتار در باند صوتی تراز می‌کند و جابه‌جایی سراسری، رانش فریم‌ریت و برش‌های میانی را بازیابی می‌کند؛ در بخش‌های پرموسیقی یا دوبلهٔ هم‌پوشان، بازبینی موردی همچنان مفید است.⁩
- ‏⁧فونت `B Koodak` تمام حروف، ارقام، علائم نگارشی، نیم‌فاصله و `RLM` فارسی را پوشش می‌دهد و libass حروف لاتین را با فونت سیستم نمایش می‌دهد.⁩

## ‏⁧مستندات⁩

- ‏⁧[دستورالعمل اسکیل اصلی](skills/revayat-subtitle/SKILL.md)⁩
- ‏⁧[دستورالعمل اسکیل همگام‌سازی صوتی](skills/revayat-subtitle-sync/SKILL.md)⁩
- ‏⁧[مرجع همگام‌سازی صوتی](skills/revayat-subtitle/references/audio-sync.md)⁩
- ‏⁧[قالب‌های زیرنویس و حالت مترجم](skills/revayat-subtitle/references/subtitle-formats.md)⁩
- ‏⁧[تایپوگرافی فارسی و مهار RTL](skills/revayat-subtitle/references/persian-typography.md)⁩
- ‏⁧[قراردادهای گردش کار](skills/revayat-subtitle/references/workflow.md)⁩
- ‏⁧[راهنمای لحن زبان مقصد](skills/revayat-subtitle/references/target-voice.md)⁩
- ‏⁧[قواعد زبان‌های مبدأ](skills/revayat-subtitle/references/source-languages.md)⁩
- ‏⁧[مسیرهای پلتفرم و مانیفست‌های پلاگین](docs/platforms.md)⁩
- ‏⁧[تاریخچهٔ تغییرات](CHANGELOG.md)⁩

## ‏⁧توسعه⁩

<div dir="ltr">

```bash
python -m pip install -r requirements-dev.txt
python -X utf8 -m pytest tests/ -q
python -X utf8 tests/check.py --render
cargo test --manifest-path rust/subsync-rs/Cargo.toml
```

</div>

گردش‌کار اصلی با هر push به `main`، تغییر PR، درخواست صف مرج و اجرای دستی شروع می‌شود. تست‌های Python نسخه‌های `3.11` و `3.14`، بررسی قالب‌های پلاگین و ASS/RTL، ابزارهای `actionlint` و `zizmor` و سلامت مخزن اجرا می‌شوند. موتور سینک و هر `3` افزونهٔ Rust در Linux، Windows و macOS ساخته و آزمایش می‌شوند؛ نبود افزونه در مرحلهٔ تطبیق رفتار، خطا محسوب می‌شود. چک `CI required` فقط پس از موفقیت تمام مراحل سبز می‌شود و انتشار امضاشده هم به همین تست‌ها وابسته است.

‏⁧گزارش‌های اجرا بدون ذخیرهٔ متن زیرنویس‌ها در پوشهٔ `logs/` ثبت می‌شوند.⁩

<a id="releases-and-tags"></a>

## نسخه‌ها و تگ‌ها

[![Latest GitHub tag](https://img.shields.io/github/v/tag/KiaroSama/Revayat-Subtitle-Skill?sort=semver&include_prereleases&label=tag&color=ea580c)](https://github.com/KiaroSama/Revayat-Subtitle-Skill/tags)

این badge از [تگ‌های واقعی مخزن](https://github.com/KiaroSama/Revayat-Subtitle-Skill/tags) به‌ترتیب نسخهٔ معنایی، با احتساب نسخه‌های پیش‌انتشار، خوانده می‌شود. با انتشار تگی که نسخهٔ بالاتری دارد و پس از تازه‌شدن کش، خودکار به‌روزرسانی می‌شود؛ تگ‌های backport نسخهٔ نمایشی را پایین نمی‌آورند. [انتشارها](https://github.com/KiaroSama/Revayat-Subtitle-Skill/releases) و [روش بررسی امضا](docs/signing/README.md) جداگانه در دسترس‌اند. نسخهٔ manifestهای پلاگین همچنان باید هنگام انتشار هماهنگ به‌روزرسانی شود.

## ‏⁧حمایت مالی⁩

‏⁧اگر این پروژه برایتان مفید است، حمایت مالی شما قدردانی می‌شود.⁩

<div dir="ltr">

| Currency | Network | Address |
| --- | --- | --- |
| Bitcoin (BTC) | Bitcoin | `bc1qmth5m03pu5hujw5xw5jmywam3jj3sqwqupesdt` |
| USDT, BNB, USDC, etc. | BEP20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| USDT, TRX, USDC, etc. | TRC20 | `TWBA3xFTqgZAeAYMxqo85xWnzvty3DcAhw` |
| Ethereum (ETH) | ERC20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| TON | TON | `UQCN8Umo_OfOWqImZetQsrNStPcmLkMAKajFyiCOhso23NDb` |
| Litecoin (LTC) | LTC | `ltc1qntqnnrunadurnw4cshv3qgspywrueyyeyngwuy` |
| Solana (SOL) | Solana | `7B2wkczUjmkDhETwQuknBL8sUsbuV7nErxc317TmQuwR` |
| Polygon (POL) | Polygon | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |

</div>

‏⁧حمایت در GitHub Sponsors: [KiaroSama](https://github.com/sponsors/KiaroSama).⁩

## ‏⁧نویسنده⁩

‏⁧نویسنده: Kiaro Sama⁩
‏⁧گیت‌هاب: [KiaroSama](https://github.com/KiaroSama)⁩

## ‏⁧مجوز⁩

‏⁧[مجوز عمومی همگانی گنو نسخهٔ ۳ یا جدیدتر](LICENSE). حق نشر © ۲۰۲۶ Kiaro Sama.⁩

‏⁧فونت همراه `B Koodak` Bold (`skills/revayat-subtitle/assets/fonts/BKOODB.TTF`) حقوق و انتساب اصلی برنا رایانه را در [NOTICE.txt](skills/revayat-subtitle/assets/fonts/NOTICE.txt) حفظ می‌کند. FFmpeg و رسانه‌های کاربر مجوزهای خودشان را دارند و در این مخزن توزیع نمی‌شوند.⁩

</div>

## Donate

If this project helps you, donations are appreciated.

| Currency | Network | Address |
| --- | --- | --- |
| Bitcoin (BTC) | Bitcoin | `bc1qmth5m03pu5hujw5xw5jmywam3jj3sqwqupesdt` |
| USDT, BNB, USDC, etc. | BEP20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| USDT, TRX, USDC, etc. | TRC20 | `TWBA3xFTqgZAeAYMxqo85xWnzvty3DcAhw` |
| Ethereum (ETH) | ERC20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| TON | TON | `UQCN8Umo_OfOWqImZetQsrNStPcmLkMAKajFyiCOhso23NDb` |
| Litecoin (LTC) | LTC | `ltc1qntqnnrunadurnw4cshv3qgspywrueyyeyngwuy` |
| Solana (SOL) | Solana | `7B2wkczUjmkDhETwQuknBL8sUsbuV7nErxc317TmQuwR` |
| Polygon (POL) | Polygon | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
