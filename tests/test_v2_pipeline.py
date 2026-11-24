"""Persian ASS conversion and editorial slot contracts."""

import importlib
from pathlib import Path
import re
from types import SimpleNamespace

import pytest

from check import SCRIPTS
import markup
import render
from runtime import read_json, write_json
import subtitle_formats as formats
import workflow

RLE, RLM, PDF = '‫', '‏', '‬'
ZWNJ = '‌'


@pytest.fixture(autouse=True)
def python_pipeline(monkeypatch):
    monkeypatch.setenv('REVAYAT_DISABLE_RUST', '1')


@pytest.fixture
def pipeline():
    return importlib.import_module('text_pipeline')


@pytest.mark.parametrize(('source', 'expected'), [
    ('می روم، نمی توانم؛ می شود، می کند، می خورم، می گویم، نمی دانم، می آید',
     'می~روم، نمی~توانم؛ می~شود، می~کند، می~خورم، می~گویم، نمی~دانم، می~آید'),
    ('آن ها و این هایش؛ حلقه ها، گل هایی، کتاب هایمان و کتاب هایتان و خانه هایشان',
     'آن~ها و این~هایش؛ حلقه~ها، گل~هایی، کتاب~هایمان و کتاب~هایتان و خانه~هایشان'),
    ('بزرگ تر، سریع ترین، بهترین، تر', 'بزرگ~تر، سریع~ترین، بهترین، تر'),
    ('رفته ام، خانه ای، گفته اش، ديده اند', 'رفته~ام، خانه~ای، گفته~اش، دیده~اند'),
    ('هم چنین، بی شک، هیچ وقت، یک دیگر، به جای', 'همچنین، بی~شک، هیچ~وقت، یکدیگر، به~جای'),
    ('مي روم و كتاب را به مصطفى و فاطمة می دهم', 'می~روم و کتاب را به مصطفی و فاطمه می~دهم'),
    ('می و جام؛ می، و گل؛ می روی میز است', 'می و جام؛ می، و گل؛ می روی میز است'),
    (r'{\fnفونت عربي}<i title="عربي">مي روم</i>\N\n Latin https://example.org/كتاب',
     r'{\fnفونت عربي}<i title="عربي">می~روم</i>\N\n Latin https://example.org/كتاب'),
])
def test_zwnj_is_tag_safe_and_idempotent(pipeline, source, expected):
    expected = expected.replace('~', ZWNJ)
    assert pipeline.normalize_persian_zwnj(source) == expected
    assert pipeline.normalize_persian_zwnj(expected) == expected


@pytest.mark.parametrize('text', [
    'زیرنویس از دوست', 'زيرنويس از مترجم', 'ترجمه و تنظیم', 'تنظیم و ترجمه', 'مترجم: الف',
    'تقدیم به نگاه شما', 'کانال تلگرام', 'ساب~تایتل', 'فارس~ساب', 'موویکو', 'دیجی~موویز', 'زارفیل',
    'سافت~ساب', 'اپلیکیشن', 'وب~سایت', 'shine021', 'farsisubtitle', 'worldsubtitle', 'subf2m',
    'addic7ed', 'opensubtitles', 'subscene', 'yify', 'yts', 'sync and corrected by A',
    'synced and corrected by A', 'subtitles by A', 'captioned by A', 'encoded by A',
    'Downloaded from https://example.test', 'Support us and become VIP', 'Advertise your product here',
    'www.example.test', 'http://example.test', 'https://example.test', 't.me/channel', '@test_channel',
    'Deluxe Media', 'Sous-titres : Alice', 'Sous-titrage : Alice', 'Supervision créative : Alice',
    'Vertaling: Bob', 'Creatief supervisor: Bob', '日本語字幕　山田', RLE + '<i> زیرنویس از ما </i>' + PDF,
])
def test_credit_markers(pipeline, text):
    text = text.replace('~', ZWNJ)
    assert pipeline.watermark_reason(text)
    assert pipeline.is_watermark_cue(text)


@pytest.mark.parametrize('text', ['king', 'Take the ring.', 'Hello, help me!', 'Long live the king!',
                                  'حلقه را بگیر!', 'حالا می روم خانه.', 'گفت این اپلیکیشن را پاک کنم.',
                                  'Send mail to a@example.test.', 'He is addicted to it.'])
def test_dialogue_is_not_a_watermark(pipeline, text):
    assert pipeline.watermark_reason(text) == ''
    assert not pipeline.is_watermark_cue(text)


def test_markup_preserves_color_alignment_and_breaks(pipeline):
    text = '{an8}<i><b>A</b></i><br><u>B</u>\n<s>C</s><font color="#1200F3">D</font>' + r'{\an3}'
    expected = r'{\an8}{\i1}{\b1}A{\b0}{\i0}\N{\u1}B{\u0}\N{\s1}C{\s0}{\c&HF30012&}D{\c}{\an3}'
    assert pipeline.convert_srt_markup_to_ass(text) == expected
    assert pipeline.convert_srt_markup_to_ass(expected) == expected
    assert pipeline.convert_srt_markup_to_ass(r'{\fnKeep <i> literal}x') == r'{\fnKeep <i> literal}x'


def test_wrap_balances_visible_text_without_splitting_tags_or_words(pipeline):
    text = r'{\i1}one two three four five six seven eight nine ten eleven twelve{\i0}'
    wrapped = pipeline.wrap_subtitle_lines(text)
    assert wrapped == r'{\i1}one two three four five six seven\Neight nine ten eleven twelve{\i0}'
    assert pipeline.wrap_subtitle_lines(wrapped) == wrapped
    word = 'خانه' + ZWNJ + 'ها'
    wrapped = pipeline.wrap_subtitle_lines(f'این {word} زیباست', 12)
    assert word in wrapped and r'\N' in wrapped


def test_wrap_preserves_speaker_and_short_explicit_breaks(pipeline):
    for text in (r'- first speaker\N- second speaker', r'one\Ntwo'):
        assert pipeline.wrap_subtitle_lines(text) == text
    wrapped = pipeline.wrap_subtitle_lines(r'- first speech with more words to wrap\N- second voice', 20)
    assert wrapped.endswith(r'\N- second voice')
    assert wrapped.startswith('- first speech')


@pytest.mark.parametrize('ending', list('.،؟!…:؛»)]"\'-'))
def test_rtl_anchors_logical_end_on_each_display_line(pipeline, ending):
    text = r'{\an8}- سلام' + ending + r'\N{\i1}دنیا' + ending + r'{\i0}'
    expected = (r'{\an8}' + RLE + RLM + '- سلام' + ending + RLM + PDF + r'\N{\i1}'
                + RLE + 'دنیا' + ending + RLM + PDF + r'{\i0}')
    result = pipeline.anchor_rtl_line(text)
    assert result == expected
    assert pipeline.anchor_rtl_line(result) == result
    markup.validate_bidi(result, 'ass')


def test_rtl_strips_outer_noise_but_preserves_drawings_and_ltr(pipeline):
    assert pipeline.anchor_rtl_line(PDF + RLM + 'سلام!' + RLE, 'srt') == RLE + 'سلام!' + RLM + PDF
    assert pipeline.anchor_rtl_line('سلام.\nدنیا!', 'srt').split('\n') == [
        RLE + 'سلام.' + RLM + PDF, RLE + 'دنیا!' + RLM + PDF]
    assert pipeline.anchor_rtl_line('سلام!', 'srt', 'ltr') == 'سلام!'
    assert pipeline.anchor_rtl_line('Hello!') == 'Hello!'
    drawing = r'{\p1}m 0 0 l 1 2{\p0}'
    assert pipeline.anchor_rtl_line(drawing) == drawing
    assert markup.rtl('سلام.', 'ass').endswith('.' + RLM + PDF)


def test_process_honors_flags_and_line_model(pipeline):
    assert pipeline.process_cue_text('<i>مي روم؟</i>', fix_zwnj=False, wrap_lines=False) == (
        r'{\i1}' + RLE + 'مي روم؟' + RLM + PDF + r'{\i0}')
    assert pipeline.process_cue_text('one two three four five', 'srt', 'ltr', 14) == 'one two three\nfour five'
    assert pipeline.process_cue_text('مي روم', direction='ltr') == 'مي روم'


def test_rust_delegation_can_be_disabled_per_call(pipeline, monkeypatch):
    monkeypatch.setattr(pipeline, '_subcore_rs', SimpleNamespace(
        REVAYAT_NATIVE_CONTRACT=1,
        normalize_persian_zwnj=lambda text: 'native:' + text))
    monkeypatch.delenv('REVAYAT_DISABLE_RUST')
    assert pipeline.normalize_persian_zwnj('می روم') == 'native:می روم'
    monkeypatch.setenv('REVAYAT_DISABLE_RUST', '1')
    assert pipeline.normalize_persian_zwnj('می روم') == 'می' + ZWNJ + 'روم'


def test_document_profile_and_watermark_log():
    original = formats.Document('srt', [
        formats.Cue('one', 1001, 2009, '{an8}<font color="#123456"><i>می روم.</i></font>'),
        formats.Cue('two', 3000, 5000, 'Sous-titrage : A'),
    ])
    result, removed = formats.srt_document_to_ass(original, title='Episode')
    payload = formats.serialize(result, result.cues)
    assert 'PlayResX: 1920\nPlayResY: 1080\nScaledBorderAndShadow: yes\nWrapStyle: 0' in payload
    assert ('Style: Default,B Koodak,52,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,'
            '-1,0,0,0,100,100,0,0,1,1.5,0.8,2,20,20,24,1') in payload
    assert [(row['id'], row['reason']) for row in removed] == [('two', 'studio-credit')]
    cue, = formats.parse(payload, 'ass').cues
    assert (cue.start, cue.end) == (1000, 2000)
    assert cue.text.startswith(r'{\an8}{\c&H563412&}{\i1}')
    assert formats.visible(cue.text, 'ass') == 'می' + ZWNJ + 'روم.'
    assert original.cues[0].start == 1001 and len(original.cues) == 2
    style = formats.build_ass_style(fontname='Custom', fontsize=48, outline=2.5, shadow=0)
    result, removed = formats.srt_document_to_ass(original, style_line=style, strip_watermarks=False)
    assert result.styles['Default'][1:3] == ['Custom', '48']
    assert result.styles['Default'][16:18] == ['2.5', '0']
    assert len(result.cues) == 2 and not removed


def test_donor_conversion_rejects_arbitrary_ass_and_collapsed_time():
    assert formats.srt_to_ass(formats.Cue('a', 0, 1000, r'{an8}A')).text == r'{\an8}A'
    assert formats.srt_to_ass(formats.Cue('a', 0, 1000, r'{\an8}A')).text == r'{\an8}A'
    with pytest.raises(ValueError, match='ASS control syntax'):
        formats.srt_to_ass(formats.Cue('a', 0, 1000, r'{\pos(5,5)}A'))
    with pytest.raises(ValueError, match='collapses'):
        formats.srt_document_to_ass(formats.Document('srt', [formats.Cue('b', 1, 9, 'Hello')]))


def test_translator_ass_sorts_empty_slots_and_interleaves_source_comments():
    primary = formats.Document('srt', [formats.Cue('a', 4100, 5000, 'چه خبر؟'),
                                      formats.Cue('b', 1000, 2000, '<i>می روم.</i>')])
    donor = formats.Document('srt', [formats.Cue('a', 1100, 1900, 'Covered'),
                                    formats.Cue('b', 1800, 2600, 'Quarter overlap'),
                                    formats.Cue('c', 3000, 3900, 'Missing'),
                                    formats.Cue('d', 5200, 6000, 'Addic7ed')])
    edition, stats = formats.build_translator_ass(primary, [('Dutch', donor), ('Duplicate', donor)],
                                                  include_source_comments=True)
    assert [cue.start for cue in edition.cues] == [1000, 1800, 3000, 4100]
    assert [formats.visible(cue.text, 'ass') for cue in edition.cues] == ['می' + ZWNJ + 'روم.', '', '', 'چه خبر؟']
    assert (stats['primary_cues'], stats['placeholder_slots'], stats['watermarks_removed']) == (2, 2, 2)
    payload = formats.serialize(edition, edition.cues)
    assert 'Dutch: Quarter overlap' in payload and 'Dutch: Covered' in payload
    assert re.search(r'^Comment: .*Duplicate: Missing\nDialogue: .*0:00:03\.00,0:00:03\.90,.*,$', payload, re.M)
    assert [cue.text for cue in formats.parse(payload, 'ass').cues] == [cue.text for cue in edition.cues]
    edition, _ = formats.build_translator_ass(primary, [('Dutch', donor)])
    assert 'Comment:' not in formats.serialize(edition, edition.cues)


def test_font_bundle_contains_b_koodak():
    directory = render.bundled_fonts_dir()
    assert directory is not None
    assert (directory / 'BKOODB.TTF').is_file()


def _workspace(tmp_path):
    src = tmp_path / 'dialogue.srt'
    src.write_text('1\n00:00:01,001 --> 00:00:02,009\n<i>می روم.</i>\n\n2\n'
                   '00:00:04,000 --> 00:00:05,000\nSubtitles by A\n', encoding='utf-8')
    work = tmp_path / 'work'
    workflow.prepare([src], work, 'Episode', 1, 'utf-8', None, 'fa')
    project = read_json(work / 'project.json')
    project['episodes'] = [{'id': 'S01E01', 'base': 's0001', 'sources': ['s0001'], 'comparison': 'Only release'}]
    write_json(work / 'project.json', project)
    glossary = read_json(work / 'glossary.json')
    write_json(work / 'glossary.json', {
        **glossary,
        'terms_reviewed': True,
        'research': [{'url': 'https://example.invalid/episode', 'note': 'Fixture series names'}],
    })
    sheet = work / 'worksheets' / 's0001.json'
    write_json(sheet, [{**row, 'action': 'preserve', 'reviewed': True} for row in read_json(sheet)])
    return src, work


def test_build_defaults_to_ass_and_explicit_srt_remains_loadable(tmp_path):
    _, work = _workspace(tmp_path)
    result = workflow.build(work)
    assert result['episodes'][0]['file'] == 'Sub/S01E01.ass'
    _, docs = render.load_build(Path(result['build']))
    assert [formats.visible(cue.text, 'ass') for cue in docs['S01E01'].cues] == ['می' + ZWNJ + 'روم.']
    assert result['episodes'][0]['provenance'][0]['timing']['conversion'] == 'floor-centisecond'
    srt = workflow.build(work, output_format='srt')
    assert srt['identity'] != result['identity']
    assert srt['episodes'][0]['file'] == 'Sub/S01E01.srt'
    render.load_build(Path(srt['build']))
    assert read_json(work / 'project.json')['output_format'] == 'ass'


def test_convert_cli_publishes_without_overwriting(tmp_path, capsys):
    src, _ = _workspace(tmp_path)
    spec = importlib.util.spec_from_file_location('revayat_cli', SCRIPTS / 'revayat-subtitle.py')
    cli = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cli)
    out = tmp_path / 'converted.ass'
    args = ['convert', str(src), '--out', str(out), '--font', 'Custom', '--outline', '2.5', '--shadow', '0']
    assert cli.execute(args) == 0
    document = formats.parse(out.read_text(encoding='utf-8'), 'ass')
    assert document.styles['Default'][1] == 'Custom'
    assert len(document.cues) == 1
    assert cli.execute(args) == 2
    assert 'Output already exists' in capsys.readouterr().err
