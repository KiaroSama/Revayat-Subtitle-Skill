<div dir="rtl">

# روایت زیرنویس — Revayat Subtitle

**زیرنویس انیمه را از هر زبانی به فارسی گفتاری ترجمه کنید یا زیرنویس فارسی را کامل اصلاح کنید؛ خروجی هر قسمت یک فایل بازبینی‌شده داخل `Sub.zip` است.**

اسکیلی برای Claude Code، Codex، Cursor، Kiro، Cline، Hermes، OpenCode،
Antigravity و هر ایجنتی که `SKILL.md` را می‌خواند. تمرکز آن روی بخش‌هایی است
که دقت می‌خواهند: تمام خطوط تمام نسخه‌ها، نام‌های یکسان بین فصل‌ها، پسوندهای
ژاپنی، تشخیص افکت از متن و نمایش درست فارسی و انگلیسی در رندر واقعی.

<div align="right"><a href="LICENSE">مجوز GPL-3.0</a></div>
<div align="left"><a href="README.md">English</a></div>

---

## چه چیزی آن را از یک مترجم زیرنویس معمولی جدا می‌کند

| | |
| --- | --- |
| **تمام نسخه‌ها بررسی می‌شوند** | شناسهٔ ثابت خطوط، هش ورودی‌ها و برگه‌های کامل جلوی فراموش‌شدن یک نسخه یا خط را می‌گیرند. |
| **فارسی گفتاری** | متن هر زبان مبدأ ترجمه و متن فارسی از نظر املا، معنا و روانی اصلاح می‌شود؛ خود ایجنت گفت‌وگو را در بافتش می‌خواند. |
| **نام‌ها در فصل بعد عوض نمی‌شوند** | تحقیق دربارهٔ انیمه، ثبت منابع و واژه‌نامهٔ قفل‌شده، نام‌ها و لحن شخصیت‌ها را به قسمت‌ها و فصل‌های بعد منتقل می‌کنند. |
| **پسوندهای ژاپنی حفظ می‌شوند** | سان، کون، ساما، چان و اونی-چان به آقا و خانم تبدیل نمی‌شوند. |
| **یک خروجی برای هر قسمت** | نسخهٔ مناسب ASS مبنای نمایش است؛ متن بهتر یا محتوای جاافتاده از نسخه‌های دیگر با تطبیق دقیق وارد می‌شود. |
| **RTL واقعاً دیده می‌شود** | متن یونیکد با ترتیب منطقی، نشانه‌های جهت و تصاویر FFmpeg، اشکال ترتیب فارسی/انگلیسی، علائم، نقل‌قول و بریدگی را آشکار می‌کنند. |
| **افکت با متن اشتباه نمی‌شود** | مسیرهای برداری، موقعیت، حرکت، کارائوکه و لایه‌های عمدی از متن تابلوها و دیالوگ جدا هستند. |
| **فونت انتخاب کاربر است** | اگر فونت جاسازی‌شده وجود داشته باشد، ایجنت دربارهٔ حذف یا حفظ آن سؤال می‌کند؛ مگر کاربر قبلاً برای همان کار تصمیم داده باشد. فونت جایگزین پس از حذف بررسی می‌شود. |
| **پاک‌سازی محدود و دقیق** | تبلیغات تأییدشده، خطوط خالی، کامنت‌های مخفی و استایل‌های بی‌استفاده حذف می‌شوند؛ محتوای داستان و شدت فحش و الفاظ جنسی حفظ می‌شود. |
| **خروجی شواهد خودش را بررسی می‌کند** | QA و بسته‌بندی از بررسی مشترک ورودی، زیرنویس، واژه‌نامه و تصویر استفاده می‌کنند؛ ZIP نهایی دوباره خوانده می‌شود. |

راهنمای [زبان‌های مبدأ](skills/revayat-subtitle/references/source-languages.md) برای ژاپنی، چینی، فرانسوی و اسپانیایی قواعد ویژه دارد؛ برای هر زبان دیگر تحقیق جداگانه لازم است. [گزارش تحقیق](skills/revayat-subtitle/references/research.md) منابع و تصمیم‌ها را ثبت می‌کند. پشتیبانی گردش کار، تضمین مهارت زبانی تمام ایجنت‌ها نیست.

## نصب

**Python 3.10 یا جدیدتر**، FFmpeg دارای libass و یک فونت فارسی لازم است.
اجرای Python به بستهٔ اضافی نیاز ندارد؛ فایل requirements همین موضوع را مشخص
می‌کند. FFmpeg و فونت جداگانه نصب می‌شوند.

<div dir="ltr">

```bash
git clone https://github.com/KiaroSama/Revayat-Subtitle-Skill.git
cd Revayat-Subtitle-Skill
python -m pip install -r skills/revayat-subtitle/requirements.txt
```

</div>

سپس اسکیل را در ایجنت‌های مورد استفاده نصب کنید:

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

نصب‌کننده پیش‌فرض مسیرهای کاربری موجود را انتخاب می‌کند. `--agent codex`
برای یک ایجنت و `--scope project --path PATH` برای نصب در یک پروژه است.
`--dry-run` مقصدها را نشان می‌دهد؛ `--force` پیش از جایگزینی از نصب قبلی
پشتیبان نگه می‌دارد. راه‌اندازهای Bash و PowerShell از یک پیاده‌سازی مشترک استفاده می‌کنند.

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

در هر مقصد یک پوشهٔ واقعی `revayat-subtitle` قرار می‌گیرد. برای مسیر دلخواه
از `--destination` استفاده کنید. پس از نصب ایجنت را reload کنید؛ اسکیل‌های
پروژه‌ای Hermes به تأیید اعتماد خود آن برنامه هم نیاز دارند.
[مستندات پلتفرم‌ها](docs/platforms.md) مسیرهای فعلی را ثبت می‌کند. نصب‌کننده
خودکار اعتماد اعطا نمی‌کند و پیش‌نیازها را نصب نمی‌کند.

### به‌عنوان پلاگین Claude Code

<div dir="ltr">

```text
/plugin marketplace add KiaroSama/Revayat-Subtitle-Skill
/plugin install revayat-subtitle@revayat-subtitle-marketplace
```

</div>

بسته سه فرمان `translate-subtitles`، `revayat-subtitle-resume` و
`revayat-subtitle-qa` دارد؛ Claude آن‌ها را در فضای نام `revayat-subtitle` نمایش می‌دهد.
برای ساخت یک کپی محلی از پلاگین:

<div dir="ltr">

```text
python install/install.py --plugin --destination "PATH/revayat-subtitle"
```

</div>

مانیفست استاندارد Agent Plugins و مانیفست‌های Claude/Codex/Cursor همگی از
همین درخت اسکیل استفاده می‌کنند. ورود و فعال‌سازی بسته در Codex، Kiro Powers،
Hermes و Antigravity از مدیر پلاگین خودشان انجام می‌شود. Cursor کپی محلی در
`~/.cursor/plugins/local/revayat-subtitle` را هم می‌پذیرد. در Cline و OpenCode
از نصب اسکیل استفاده کنید؛ سیستم افزونهٔ JS/TS آن‌ها محصول متفاوتی است.

### بررسی نصب

<div dir="ltr">

```bash
python skills/revayat-subtitle/scripts/revayat-subtitle.py doctor
```

</div>

`ready: true` وجود فیلترهای FFmpeg را تأیید می‌کند؛ مناسب‌بودن فونت یا نمایش
درست یک قسمت هنوز به رندر و مشاهده نیاز دارد. اگر FFmpeg در PATH نیست، مسیرش
را با `--ffmpeg` یا `REVAYAT_FFMPEG` مشخص کنید.

## استفاده

نام انیمه، فصل و جدیدبودن یا ادامهٔ مجموعه را به ایجنت بگویید:

> از revayat-subtitle استفاده کن. این فصل دوم همان انیمه است. نام‌های قبلی و
> پسوندهای ژاپنی را حفظ کن، فارسی گفتاری بنویس، فونت‌ها را نگه دار و Sub.zip
> و واژه‌نامهٔ به‌روزشده را بده.

فایل‌های ASS/SRT یا ZIP و برای ادامهٔ مجموعه، واژه‌نامهٔ قبلی را ضمیمه کنید.
در Codex از `$revayat-subtitle` استفاده کنید. در پلاگین Claude، شروع کار با
`/revayat-subtitle:translate-subtitles`، ادامه با
`/revayat-subtitle:revayat-subtitle-resume` و گزارش QA با
`/revayat-subtitle:revayat-subtitle-qa` انجام می‌شود.

### یا خودتان مرحله‌به‌مرحله اجرا کنید

<div dir="ltr">

```bash
PY=python3
S=skills/revayat-subtitle/scripts

"$PY" "$S/revayat-subtitle.py" doctor
"$PY" "$S/revayat-subtitle.py" prepare season.zip --work work/season01 --series "Anime title" --season 1
# Research names, fill glossary.json and map episodes in project.json.
# Read every cue and complete its worksheet decision and target text.
"$PY" "$S/revayat-subtitle.py" build --work work/season01
# Set BUILD to the printed build path.
"$PY" "$S/revayat-subtitle.py" render --build "$BUILD" --episode S01E01 --video episode01.mkv
# Inspect every PNG and record observations in its review file.
"$PY" "$S/revayat-subtitle.py" qa --build "$BUILD"
"$PY" "$S/revayat-subtitle.py" package --build "$BUILD" --out out/Sub.zip
```

</div>

در PowerShell، `$PY = 'python'` و `$S = 'skills/revayat-subtitle/scripts'`
را تنظیم و فرمان را با `& $PY` اجرا کنید.
[قرارداد فایل‌ها](skills/revayat-subtitle/references/workflow.md) جزئیات برگه‌ها را توضیح می‌دهد.

**ورودی:** ASS، SRT، پوشه و ZIP. **خروجی:** یک زیرنویس برای هر قسمت در پوشهٔ
`Sub` داخل ZIP، به‌همراه واژه‌نامه‌ای جدا. نام‌ها مانند `S01E01.ass`،
`S01OVA01.ass` و `S02E01.ass` هستند. قسمت‌های صرفاً SRT همان قالب را نگه می‌دارند.
**زبان مقصد:** فارسی، مگر کاربر زبان دیگری بخواهد.

## معماری

<div dir="ltr">

```text
ASS / SRT / ZIP
       |
       v
prepare -> immutable sources + project.json + complete worksheets
       |                             |
       |                     research + locked glossary
       v                             |
compare releases <------------------+
       |
       v
agent reads, translates and records every cue decision
       |
       v
build -> timing sort, cleanup, RTL, one file per episode
       |
       v
render -> FFmpeg PNGs -> agent inspection and observations
       |
       v
qa -> source, edition, glossary and image evidence agree
       |
       v
package -> Sub.zip, read back and checked
```

</div>

معنا و قضاوت تصویری بر عهدهٔ ایجنت است؛ ساختار فایل و بررسی شواهد را اسکریپت‌ها
انجام می‌دهند. فایل اصلی بازنویسی نمی‌شود و تغییر برگهٔ ترجمه، شناسهٔ ساخت و
شواهد تصویری تازه می‌خواهد.

| ماژول | نقش |
| --- | --- |
| `subtitle_formats.py` | مدل ASS/SRT، خواندن و نوشتن دقیق، جداسازی رسم از متن و نشانه‌های جهت |
| `workflow.py` | فهرست ورودی، برگه‌ها، واژه‌نامه، ادغام نسخه‌ها و هویت ساخت |
| `render.py` | تصاویر FFmpeg، دروازهٔ مشترک QA و خروجی ZIP بررسی‌شده |
| `runtime.py` | UTF-8، هش، اجرای محدود فرایندها، پوشهٔ موقت و لاگ اجرا |
| `revayat-subtitle.py` | ورودی واحد خط فرمان برای تمام مراحل |
| `install/install.py` | مسیر عامل‌ها و نصب اسکیل/پلاگین از فهرست مجاز فایل‌ها |

## آنچه صادقانه باید گفت

- **برگهٔ کامل اثبات فهم متن نیست.** دقت معنایی و مشاهدهٔ تصویر همچنان کار ایجنت است.
- **رندر خنثی آزمون هماهنگی ویدئو نیست.** فریم ویدئوی متناظر کنتراست صحنه را هم نشان می‌دهد، اما به‌تنهایی همگامی صدا و تصویر را ثابت نمی‌کند.
- **استایل‌ها همیشه قابل جابه‌جایی نیستند.** مارک‌آپ پیچیده، بوم، تدوین و فونت‌های ناسازگار به تطبیق صریح نیاز دارند.
- **رفتار پخش‌کننده‌ها متفاوت است.** بعضی خطوط دشوار به تنظیم مخصوص libass نیاز دارند و نمی‌توان آن را پشتیبانی عمومی VSFilter نامید.
- **آزمون واقعی محدود بوده است.** ۵۱۸۸ رویداد در چهار فایل بررسی ساختاری شدند؛ ۴۰ خط نمونه و ۱۰ تصویر از ویدئوی واقعی بازبینی شدند. این گواهی کیفیت یک فصل کامل نیست.

[محدودهٔ آزمون](docs/verification.md) ثبت شده است. فایل‌های خصوصی توزیع نمی‌شوند.
ASSهای واقعی انتخاب‌شده فونت جاسازی‌شده نداشتند؛ حفظ و حذف دادهٔ فونت با نمونه‌های
ساختاریِ تألیفی بررسی می‌شود.

## مستندات

اسکیل در مرحلهٔ لازم به این منابع مراجعه می‌کند:

- [SKILL.md](skills/revayat-subtitle/SKILL.md) — نُه مرحله و معیار پایان هر مرحله.
- [translation-policy.md](skills/revayat-subtitle/references/translation-policy.md) — فارسی گفتاری، حفظ کامل معنا و حذف‌های محدود.
- [persian-typography.md](skills/revayat-subtitle/references/persian-typography.md) — RTL، متن ترکیبی، نقل‌قول، فونت و تصویر.
- [glossary-and-voice.md](skills/revayat-subtitle/references/glossary-and-voice.md) — تحقیق، اسامی و تداوم فصل‌ها.
- [release-selection.md](skills/revayat-subtitle/references/release-selection.md) — مقایسهٔ نسخه‌ها و هویت قسمت.
- [subtitle-formats.md](skills/revayat-subtitle/references/subtitle-formats.md) — قالب‌ها، کامنت، استایل، رسم و حذف اختیاری فونت.
- [workflow.md](skills/revayat-subtitle/references/workflow.md) — قرارداد پوشهٔ کار، برگه‌ها و بازبینی.
- [troubleshooting.md](skills/revayat-subtitle/references/troubleshooting.md) — خطاها، بازیابی و لاگ اجرا.

## توسعه

<div dir="ltr">

```bash
python tests/check.py
python tests/check.py --render
python evaluation/score.py --answers my-answers.json
```

</div>

رانر مبتنی بر کتابخانهٔ استاندارد، زمان اجرای فرایند را محدود و خط فرمان و
نصب‌کنندهٔ واقعی را بررسی می‌کند. CI روی Linux، macOS و Windows اجرا می‌شود؛
Linux رندر libass و پیش‌نمایش ویدئوی ۱۰بیتی را هم بررسی می‌کند. CodeQL کد Python
و گردش‌کارها را تحلیل می‌کند؛ Dependency Review برای PRهاست و Dependabot
اکشن‌ها و به‌روزرسانی‌های امنیتی را مدیریت می‌کند.

[بخش ارزیابی](evaluation/README.md) نمونه‌های زبانی کوتاه و تألیفی دارد. پاسخ‌های
پذیرفته‌شدهٔ شناخته‌شده تشخیص داده می‌شوند؛ بیان تازه برای بازبینی انسانی گزارش
می‌شود، نه اینکه خودکار ترجمهٔ غلط نامیده شود یا نمرهٔ ساختگی بگیرد.

**هر ایجنت استفاده‌کننده موظف است هنگام تحقیق، ترجمه، اصلاح، ادغام و بازبینی،
لاگ واقعی بنویسد و آن را کنار فایل‌های ترجمه‌شده قرار دهد.** لاگ فقط پیام چت
یا لاگ اسکریپت نیست؛ محل، قالب و محتوا در قواعد اسکیل مشخص شده‌اند.

لاگ اجرایی اسکریپت‌ها نیز برای هر اجرا فایل UTF-8 جدا با زمان UTC می‌سازد و
متن زیرنویس را ثبت نمی‌کند. جزئیات در
[لاگ اجرا](skills/revayat-subtitle/references/troubleshooting.md#cli-execution-logs) آمده است.

## حمایت مالی

اگر این پروژه برایتان مفید است، می‌توانید از توسعهٔ آن حمایت کنید.

</div>

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

<div dir="rtl">

حمایت در گیت‌هاب: [KiaroSama](https://github.com/sponsors/KiaroSama).

## نویسنده

نویسنده: Kiaro Sama

گیت‌هاب: [KiaroSama](https://github.com/KiaroSama)

## مجوز

[GNU General Public License v3.0 or later](LICENSE). Copyright (C) 2026 Kiaro Sama.

این برنامه نرم‌افزار آزاد است و می‌توانید آن را مطابق نسخهٔ ۳ مجوز GNU GPL
یا به انتخاب خود نسخه‌ای بعدی، بازتوزیع یا اصلاح کنید. برنامه بدون هیچ ضمانتی،
از جمله ضمانت ضمنی قابلیت فروش یا مناسب‌بودن برای هدفی خاص، ارائه می‌شود.
جزئیات در متن مجوز است.

همین مجوز همراه نصب مستقل اسکیل و پلاگین قرار می‌گیرد. FFmpeg، فونت‌ها و
رسانه‌های پردازش‌شده مجوزهای خودشان را دارند و در این مخزن توزیع نمی‌شوند.

</div>
