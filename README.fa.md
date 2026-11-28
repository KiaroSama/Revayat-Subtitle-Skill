<div dir="rtl" lang="fa">

<div align="center">

# ‏⁧روایت زیرنویس⁩

[![GitHub Actions CI](https://img.shields.io/github/actions/workflow/status/KiaroSama/Revayat-Subtitle-Skill/ci.yml?style=flat-square&label=CI&color=1d4ed8)](.github/workflows/ci.yml)
[![Repository license](https://img.shields.io/github/license/KiaroSama/Revayat-Subtitle-Skill?style=flat-square&color=2563eb)](LICENSE)
[![Latest published release](https://img.shields.io/github/v/release/KiaroSama/Revayat-Subtitle-Skill?style=flat-square&color=ea580c)](https://github.com/KiaroSama/Revayat-Subtitle-Skill/releases)
[![Latest Git tag](https://img.shields.io/github/v/tag/KiaroSama/Revayat-Subtitle-Skill?style=flat-square&label=Tag&color=c2410c)](#releases-and-tags)
[![Rust workspace tests](https://img.shields.io/badge/Tests-Rust%20Workspace-0d9488?style=flat-square)](#verification)
[![Pinned Rust version](https://img.shields.io/badge/Rust-1.100.0-8b5cf6?style=flat-square)](rust-toolchain.toml)
[![64-bit Windows and Linux](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20(64--bit)-0891b2?style=flat-square)](docs/platforms.md#supported-platforms)
[![Install with the Rust CLI](https://img.shields.io/badge/Install-Rust%20CLI-4f46e5?style=flat-square)](#install)
[![English and Persian documentation](https://img.shields.io/badge/Docs-English%20%7C%20Persian-0284c7?style=flat-square)](#documentation)
[![GitHub repository size](https://img.shields.io/github/repo-size/KiaroSama/Revayat-Subtitle-Skill?style=flat-square&color=ca8a04)](#repository-layout)
[![FFmpeg and libass for rendering](https://img.shields.io/badge/Render-FFmpeg%20%7C%20libass-334155?style=flat-square)](#media-tools)
[![ASS with bundled B Koodak](https://img.shields.io/badge/Default%20output-ASS%20%7C%20B%20Koodak-15803d?style=flat-square)](#subtitle-formats)
[![Two standalone binaries](https://img.shields.io/badge/Binaries-2-7c3aed?style=flat-square)](#command-reference)
[![Six translation stages](https://img.shields.io/badge/Translation-6%20stages-b45309?style=flat-square)](#translation-workflow)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

</div>

[English](README.md)

‏⁧روایت زیرنویس اسکیلی برای ترجمه و بازبینی زیرنویس است. با ابزارهایش می‌توانید فایل‌های ASS و SRT را تبدیل کنید، نمایش زیرنویس را روی تصویر ببینید و زمان هر خط را با دیالوگ هماهنگ کنید. زبان پیش‌فرض ترجمه فارسی است. فایل ZIP نهایی زیرنویس‌های بازبینی‌شده را دارد و واژه‌نامهٔ مجموعه جداگانه تحویل داده می‌شود.⁩

‏⁧نسخهٔ `3.0.5` دو برنامهٔ مستقل دارد: `revayat-subtitle` و `subsync`. کد پروژه به‌طور کامل با Rust `1.100.0` و ویرایش `2024` نوشته شده و هیچ اسکریپتی به زبان Python، PowerShell یا Bash همراهش نیست. انتشار و CI فقط ویندوز و لینوکس ۶۴ بیتی را روی معماری‌های x64 و arm64 پوشش می‌دهند.⁩

<a id="install"></a>

## ‏⁧نصب⁩

‏⁧هر دو برنامه را متناسب با سیستم‌عامل و معماری خود از [صفحهٔ انتشار گیت‌هاب](https://github.com/KiaroSama/Revayat-Subtitle-Skill/releases) بگیرید و پیش از اجرا [امضاهایشان را بررسی کنید](docs/signing/README.md). نام فایل‌های انتشار شامل تگ و معماری است. دو برنامه را با نام‌هایی که در نمونه‌های زیر می‌بینید کنار هم بگذارید و پوشه‌شان را به `PATH` ایجنت اضافه کنید.⁩

‏⁧خود برنامه‌ها مستقیم اجرا می‌شوند. برای نصب اسکیل، نسخه‌ای از مخزن با همان تگ بگیرید. در پوشهٔ اصلی مخزن، اول `revayat-subtitle install --dry-run` را اجرا کنید و پس از بررسی مسیرها، `revayat-subtitle install` را بزنید. نصب‌کننده فایل‌های اسکیل، فرمان‌ها و مانیفست‌ها را از همان مخزن می‌خواند.⁩

### ‏⁧ساخت از روی کد منبع⁩

‏⁧در پوشهٔ اصلی مخزن، با Rust `1.100.0` و پیونددهندهٔ مناسب معماری خود، هر دو برنامه را بسازید. سپس خروجی Cargo را در `bin/` کپی کنید و پیش از نصب، مسیرهایی را که قرار است تغییر کنند ببینید.⁩

<div dir="ltr">

Windows (PowerShell 7): `bin/revayat-subtitle.exe`, `bin/subsync.exe`

```powershell
cargo build --workspace --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
New-Item -ItemType Directory -Force -Path bin | Out-Null
Copy-Item -LiteralPath target/release/revayat-subtitle.exe, target/release/subsync.exe -Destination bin/
./bin/revayat-subtitle.exe install --dry-run
```

Linux (POSIX shell): `bin/revayat-subtitle`, `bin/subsync`

```sh
cargo build --workspace --release --locked &&
  mkdir -p bin &&
  cp target/release/revayat-subtitle target/release/subsync bin/ &&
  ./bin/revayat-subtitle install --dry-run
```

</div>

‏⁧بعد از بررسی مسیرها، فرمان `install` را بدون `--dry-run` تکرار کنید. نصب‌کننده به‌طور پیش‌فرض پوشه‌های کاربری موجود را انتخاب می‌کند. برای نصب فقط در Codex از `revayat-subtitle install --agent codex` و برای نصب در پروژهٔ فعلی از `revayat-subtitle install --agent codex --scope project --path .` استفاده کنید. گزینهٔ `--force` پیش از جایگزینی نصب موجود از آن نسخهٔ پشتیبان می‌گیرد. `bin/` را در `PATH` میزبان بگذارید و ایجنت را دوباره باز کنید. [مسیرهای نصب و بازیابی](docs/platforms.md#agent-installation) را هم ببینید.⁩

‏⁧پیش از بازگشت به برنامه‌های قدیمی، نصب‌کنندهٔ سازگار با سوابق بازیابی و نسخه‌های پشتیبان موجود را نگه دارید؛ [راهنمای سازگاری هنگام بازگشت نسخه](docs/platforms.md#installer-downgrade-compatibility) را ببینید.⁩

### ‏⁧نصب افزونه⁩

‏⁧فایل‌های اسکیل در پنج قالب مانیفست عرضه می‌شوند: Claude Code، Cursor، Codex، Antigravity و `Agent Plugins 1.0`. برای نصب در Claude Code این دو فرمان را وارد کنید:⁩

<div dir="ltr">

```text
/plugin marketplace add KiaroSama/Revayat-Subtitle-Skill
/plugin install revayat-subtitle@revayat-subtitle-marketplace
```

</div>

‏⁧این افزونه فرمان‌های `translate-subtitles`، `revayat-subtitle-resume`، `revayat-subtitle-qa` و `revayat-subtitle-sync` را در فضای نام `revayat-subtitle` اضافه می‌کند. ایجنت باید به هر دو برنامه دسترسی داشته باشد. [مسیر مانیفست‌ها](docs/platforms.md#plugin-manifests) برای هر میزبان آمده است.⁩

<a id="media-tools"></a>

## ‏⁧ابزارهای صوت و تصویر⁩

‏⁧برای رندر و استخراج صدا، `ffmpeg` و `ffprobe` باید از `PATH` در دسترس باشند. رندر به نسخه‌ای از FFmpeg نیاز دارد که با `libass` و فیلتر `subtitles` ساخته شده باشد. `revayat-subtitle doctor` پیش‌نیازها را بررسی می‌کند. حتی با نتیجهٔ `ready: true`، زیرنویس رندرشده را روی تصویر ببینید.⁩

‏⁧با `revayat-subtitle sync` و `subsync`، هر دیالوگ را جداگانه با صدای همان دیالوگ هماهنگ می‌کنید. دیالوگ‌های پشت‌سرهم کنار هم می‌مانند و زیرنویس، در صورت امکان، به‌طور پیش‌فرض ۵۰ میلی‌ثانیه پیش از شروع گفتار ظاهر می‌شود (`--lead-in-ms 50`). تنظیم پایان دیالوگ به‌طور پیش‌فرض تا ۶۰۰ میلی‌ثانیه مجاز است (`--end-adjust-ms 600`). اگر فاصلهٔ کافی باشد، زیرنویس حدود ۲۵۰ میلی‌ثانیه پس از گفتار می‌ماند و دست‌کم حدود ۹۰۰ میلی‌ثانیه برای خواندن روی تصویر است. در صدای فراگیر ۵٫۱ یا ۷٫۱، برنامه کانال میانی جلو (`FC`) را خودکار جدا می‌کند و در صورت نیاز صدا را تک‌کاناله می‌کند.⁩

‏⁧[راهنمای همگام‌سازی با FFmpeg](skills/revayat-subtitle/ffmpeg/SKILL.md) از `ffmpeg-skill` نسخهٔ `4.7.1` اقتباس شده و توضیح می‌دهد چطور نسخهٔ ویدئو و ترک دیالوگ را انتخاب کنید. اگر ویدئو چند ترک صدا دارد، ترک دلخواه را به‌صورت WAV تک‌کانالهٔ PCM با نرخ ۱۶ کیلوهرتز استخراج کنید و با `--wav` به برنامه بدهید. زیرنویس اصلی را نگه دارید و نتیجه را در ابتدا، میانه، پایان و دو طرف هر نقطهٔ برش گزارش‌شده ببینید.⁩

<div dir="ltr">

```text
revayat-subtitle sync --video "Episode 01.mkv" --in "original.ass" --out "aligned.ass" --json-report "sync-report.json"
subsync --wav "Episode 01 dialogue.wav" --in "original.ass" --out "aligned-wav.ass" --json-report "sync-wav-report.json"
```

</div>

<a id="subtitle-formats"></a>

## ‏⁧قالب‌های زیرنویس⁩

‏⁧قالب پیش‌فرض فارسی ASS از فونت `B Koodak` با اندازهٔ ۱۱۰ در قاب ۱۹۲۰×۱۰۸۰ استفاده می‌کند. ضخامت دورخط و سایه هر دو ۲ است و فاصلهٔ متن از لبه‌های چپ، راست و پایین ۱۰ است. متن پیش‌فرض معمولی است؛ اگر سبک `Default` در فایل ASS ورودی پررنگ باشد، مبدل آن را حفظ می‌کند. برای تغییر این رفتار از `--bold` یا `--no-bold` استفاده کنید. هر خط فارسی با `RLM` و `RLI` شروع می‌شود و با `PDI` پایان می‌یابد. مبدل علامت‌های جهت‌دهیِ نامتوازن را حذف می‌کند، `RLM` میان حروف را به نیم‌فاصله تبدیل می‌کند و نشانه‌گذاریِ وارونه را از ابتدای خط به پایان آن می‌برد. مبدل سبک‌ها و جلوه‌های پشتیبانی‌شدهٔ ASS را نگه می‌دارد، حروف و نیم‌فاصله‌های فارسی را اصلاح می‌کند و خط را از جایی می‌شکند که عبارت یا تگ از هم نپاشد. حذف نوشته‌های تبلیغاتیِ مشخص را ثبت می‌کند و دیالوگ‌های مبهم را دست نمی‌زند. برای نگه‌داشتن این نوشته‌ها از `--keep-watermarks` استفاده کنید.⁩

<div dir="ltr">

```text
revayat-subtitle convert "episode.srt" --out "episode.ass"
revayat-subtitle translator-ass "base.ass" --donor "donor.srt" --source-comments --out "worksheet.ass"
```

</div>

‏⁧در حالت مترجم، گروه‌های هم‌پوشان نسخهٔ اصلی سر جای خود می‌مانند و زمان‌های خالی از نسخهٔ کمکی وارد می‌شوند. متن مبدأ را هم می‌توانید در توضیحاتی نگه دارید که روی تصویر نمایش داده نمی‌شوند. اگر خروجی SRT می‌خواهید، `--output-format srt` را هنگام ورود فایل‌ها مشخص کنید تا بازبینی و ساخت بر همان قالب انجام شوند. [جزئیات قالب و حروف‌چینی](skills/revayat-subtitle/references/subtitle-formats.md) و [اطلاعیهٔ فونت](skills/revayat-subtitle/assets/fonts/NOTICE.txt) را ببینید.⁩

<a id="translation-workflow"></a>

## ‏⁧گردش کار ترجمه⁩

‏⁧هماهنگ‌کننده فایل‌های ASS یا SRT، پوشه یا فایل ZIP را وارد محیط کار می‌کند، شمارهٔ قسمت‌ها را تطبیق می‌دهد و واژه‌نامه را پیش از سپردن کار به ایجنت‌ها نهایی می‌کند. فایل‌های اصلی و شناسهٔ خطوط تغییر نمی‌کنند.⁩

1. ‏⁧ترجمهٔ اولیه: مترجم هر خط را با توجه به صحنه ترجمه می‌کند و موارد حل‌نشده را علامت می‌زند.⁩
2. ‏⁧حروف‌چینی و چیدمان: ایجنت جای متن و جلوه‌ها را حفظ می‌کند و شکست سطر، پوشش فونت و علائم راست‌به‌چپ را می‌سنجد.⁩
3. ‏⁧بازبینی متن: بازبین مستقل دقت ترجمه، لحن دیالوگ و یکدستی نام‌ها را بررسی و تصمیم‌هایش را برای همان نسخهٔ کاربرگ ثبت می‌کند.⁩
4. ‏⁧اعمال ترجمه: ایجنت تصمیم‌های تأییدشده را اجرا می‌کند، فایل را می‌سازد و اگر خواسته باشید، زمان‌بندی را با صدا تطبیق می‌دهد.⁩
5. ‏⁧بازبینی دوبارهٔ فایل و رفع ایراد: ایجنت ایرادها را رفع می‌کند، دوباره خروجی می‌گیرد و تصاویر تازهٔ همان نسخه را می‌بیند.⁩
6. ‏⁧بازبینی نهایی و تحویل: بازبین مستقل نسخه و شواهدش را تطبیق می‌دهد و برای هر قسمت یک زیرنویس در پوشهٔ `Sub/` داخل `Sub.zip` می‌گذارد.⁩

‏⁧راهنمای همراه [humanizer v4.0.0](skills/revayat-subtitle/voice/humanizer/SKILL.md) به ویراستار کمک می‌کند عبارت‌های قالبی را بدون دست‌بردن در معنا عوض کند. [راهنمای فارسی](skills/revayat-subtitle/voice/persian/SKILL.md) به لحن گفتاری، نیم‌فاصله، حروف فارسی و نشانه‌گذاری می‌پردازد. [راهنماهای لحن زبان مقصد](skills/revayat-subtitle/references/target-voice.md) انگلیسی و روسی را هم پوشش می‌دهند.⁩

‏⁧برای بازکردن محیط کار:⁩

<div dir="ltr">

```text
revayat-subtitle prepare "season.zip" --work "work/season01" --series "Anime title" --season 1
revayat-subtitle status --work "work/season01"
```

</div>

‏⁧پس از تأیید تصمیم‌های بازبینی، فایل را بسازید. خروجی را در محیط کار اصلی نگه دارید و به‌جای `BUILD`، مسیری را بگذارید که فرمان ساخت گزارش کرده است:⁩

<div dir="ltr">

```text
revayat-subtitle build --work "work/season01"
revayat-subtitle render --build "BUILD" --episode S01E01 --video "episode01.mkv"
```

</div>

‏⁧تصاویر PNG را ببینید و مشاهداتتان را ثبت کنید. پس از تأیید نسخهٔ فعلی و شواهدش در کنترل کیفیت، بسته را در مسیر تازه‌ای بنویسید:⁩

<div dir="ltr">

```text
revayat-subtitle qa --build "BUILD"
revayat-subtitle package --build "BUILD" --out "out/Sub.zip"
```

</div>

‏⁧مرحله‌ای که اجرا نشده، باز می‌ماند. اگر ویرایش فایل نسخهٔ تازه‌ای ساخت، تصاویر را دوباره بگیرید و تأییدهای بصری را از نو ثبت کنید. [روش کار شش‌مرحله‌ای](skills/revayat-subtitle/references/multi-agent-workflow.md) شیوهٔ تحویل کار بین ایجنت‌ها و ادامه‌دادن از آخرین مرحلهٔ تأییدشده را توضیح می‌دهد.⁩

<a id="command-reference"></a>

## ‏⁧مرجع فرمان‌ها⁩

‏⁧برنامهٔ `revayat-subtitle` شانزده زیرفرمان دارد. برای دیدن گزینه‌های هرکدام از `revayat-subtitle <subcommand> --help` استفاده کنید. هر دو برنامهٔ `revayat-subtitle` و `subsync` گزینه‌های `--help` و `--version` را دارند.⁩

| ‏⁧زیرفرمان⁩ | ‏⁧کاربرد⁩ |
| --- | --- |
| `doctor` | ‏⁧بررسی ابزارهای صوت و تصویر و پشتیبانی از libass⁩ |
| `prepare` | ‏⁧ورود ASS، SRT، پوشه یا ZIP و ساخت کاربرگ بازبینی⁩ |
| `build` | ‏⁧بررسی تصمیم‌های بازبینی و ساخت نسخهٔ خروجی⁩ |
| `convert` | ‏⁧تبدیل SRT یا ASS به ASS مناسب فارسی⁩ |
| `translator-ass` | ‏⁧اجرای `convert --translator-mode` برای ساخت کاربرگ ASS با زمان‌های کمکی⁩ |
| `sync` | ‏⁧هماهنگ‌کردن ASS یا SRT با صدای ویدئو یا WAV متناظر⁩ |
| `render` | ‏⁧رندر نمونه‌ها با FFmpeg و بررسی فایل‌های PNG⁩ |
| `qa` | ‏⁧بررسی آماده‌بودن نسخهٔ بازبینی‌شده برای تحویل، بدون تغییر فایل‌ها⁩ |
| `package` | ‏⁧ساخت فایل ZIP تحویل بدون جایگزینی فایل موجود⁩ |
| `status` | ‏⁧گزارش کارهای باقی‌مانده و شواهد نسخهٔ فعلی، بدون تغییر محیط کار⁩ |
| `handoff` | ‏⁧سنجش پیشنهادهای بازبینی برای نسخهٔ مشخص، بدون اعمال آن‌ها⁩ |
| `supervise` | ‏⁧اجرای فرایند فرزند با نظارت و محدودیت روی کل درخت فرایندها⁩ |
| `install` | ‏⁧نصب اسکیل یا بازیابی نصب نیمه‌تمام⁩ |
| `lint` | ‏⁧بررسی یا اصلاح رسم‌الخط و سبک متن فارسی⁩ |
| `lookup` | ‏⁧جست‌وجو در واژه‌نامه و جدول اصطلاحات همراه اسکیل⁩ |
| `score` | ‏⁧امتیازدهی به ورودی براساس معیارهای بازبینی⁩ |

‏⁧برنامهٔ مستقل `subsync` زمان زیرنویس‌های SRT و ASS را با صدا تطبیق می‌دهد و می‌تواند گزارش JSON بنویسد. در `status` و `handoff`، کد خروج `0` یعنی کار کامل است، `1` یعنی داده معتبر است و کار باقی مانده، و `2` یعنی داده نامعتبر است یا به نسخهٔ فعلی تعلق ندارد. [جزئیات وضعیت و تحویل کار](skills/revayat-subtitle/references/workflow.md#read-only-status-and-editorial-handoffs) را ببینید.⁩

## ‏⁧گزارش اجرا⁩

‏⁧برنامهٔ `revayat-subtitle` گزارش UTF-8 را با نام `logs/revayat-subtitle_YYYY-MM-DD_HH-mm-ss_UTC.log` ثبت می‌کند. اگر این نام از قبل وجود داشته باشد، به نام فایل تازه پسوند می‌دهد. قالب هر سطر `[YYYY-MM-DD HH:mm:ss UTC] [LEVEL] [COMPONENT] Message` است. گزارش شامل پیشرفت، خطا و وضعیت خروج است و متن زیرنویس یا اطلاعات محرمانه را ثبت نمی‌کند. سطح‌ها `DEBUG`، `INFO`، `WARNING`، `ERROR` و `CRITICAL` هستند. با `--log-level` می‌توانید جزئیات بیشتری بگیرید. گزارش‌ها خودکار پاک نمی‌شوند. قبل از پیوست گزارش به گزارش خطا، مسیرهای محلی داخل آن را بررسی کنید. گزارش فعالیت ایجنت‌ها جداست. برنامهٔ مستقل `subsync` نتیجهٔ JSON را در stdout و خطاها را در stderr می‌نویسد؛ برای ذخیرهٔ نتیجه از `--json-report` و برای ثبت گزارش عملیاتی از `revayat-subtitle sync` استفاده کنید.⁩

<a id="verification"></a>

## ‏⁧آزمایش و بررسی⁩

<div dir="ltr">

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

</div>

‏⁧آزمون‌های CI هر دو برنامه را روی ویندوز و لینوکس، برای x64 و ARM64، می‌سازند و آزمون‌های فضای کاری را با FFmpeg و libass روی هر ۴ هدف اجرا می‌کنند. برنامه‌های Rust در مسیر Windows ARM64 به‌صورت بومی اجرا می‌شوند و FFmpeg تأییدشدهٔ x64 از شبیه‌سازی `Windows 11` استفاده می‌کند. [دامنهٔ آزمون‌ها](docs/verification.md) جزئیات هر مسیر را توضیح می‌دهد. آزمون Rust یا بررسی PNG دقت ترجمه و زمان‌بندی در پخش‌کنندهٔ شما را تأیید نمی‌کند؛ خود زیرنویس را بخوانید و آن را با رسانهٔ متناظر پخش کنید.⁩

<a id="documentation"></a>

## ‏⁧مستندات⁩

- ‏⁧[اسکیل اصلی](skills/revayat-subtitle/SKILL.md) و [فایل‌ها و فرمان‌های گردش کار](skills/revayat-subtitle/references/workflow.md)⁩
- ‏⁧[مرجع همگام‌سازی صدا](skills/revayat-subtitle/references/audio-sync.md) و [حروف‌چینی فارسی](skills/revayat-subtitle/references/persian-typography.md)⁩
- ‏⁧[قواعد زبان مبدأ](skills/revayat-subtitle/references/source-languages.md) و [معیارهای ارزیابی](evaluation/rubric.md)⁩
- ‏⁧[مستندات انگلیسی](README.md) و [تاریخچهٔ تغییرات](CHANGELOG.md)⁩

<a id="repository-layout"></a>

## ‏⁧ساختار مخزن⁩

- ‏⁧`crates/`: کد Rust برای خط فرمان، پردازش زیرنویس و همگام‌سازی با صدا.⁩
- ‏⁧`skills/` و `commands/`: دستورهای ایجنت، راهنمای گردش کار و لحن، و فایل‌های همراه.⁩
- ‏⁧`docs/`: راهنمای پلتفرم‌ها، آزمون‌ها و اعتبارسنجی امضا.⁩
- ‏⁧`bin/`: برنامه‌هایی که روی دستگاه خود می‌سازید و در Git ثبت نمی‌شوند.⁩

<a id="releases-and-tags"></a>

## ‏⁧انتشارها و تگ‌ها⁩

‏⁧نشان انتشار و نشان تگ خودکار به‌روز می‌شوند: اولی با [انتشارهای گیت‌هاب](https://github.com/KiaroSama/Revayat-Subtitle-Skill/releases) و دومی با [تگ‌های گیت](https://github.com/KiaroSama/Revayat-Subtitle-Skill/tags). تگ‌ها ممکن است پیش از انتشار فایل‌های نسخه در فهرست دیده شوند.⁩

## Author

Kiaro Sama ([KiaroSama](https://github.com/KiaroSama)) · [Kiaro.Sama.Dev@gmail.com](mailto:Kiaro.Sama.Dev@gmail.com)

## License

‏⁧حق نشر © ۲۰۲۶ Kiaro Sama. پروژه با [مجوز عمومی همگانی گنو نسخهٔ ۳ یا جدیدتر](LICENSE) منتشر می‌شود.⁩

</div>

## Donate

‏⁧اگر روایت زیرنویس به کارتان آمده، می‌توانید از پروژه حمایت کنید.⁩

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

‏⁧حمایت از راه GitHub Sponsors: [KiaroSama](https://github.com/sponsors/KiaroSama).⁩
