#include "MarkdownHtml.h"

#include <QFontDatabase>
#include <QObject>
#include <QStringList>
#include <QVector>

#include <md4c.h>
extern "C" {
#include <entity.h>
}

namespace {
// github-markdown-css 5.8.1 comment metrics at a 14 px base font (rem = 16 px).
constexpr int BaseSize = 14;
constexpr int BlockGap = 16, HeadingGap = 24, RuleGap = 24;
constexpr int LineHeight = 21, CodeLineHeight = 23;

QColor over(const QColor &top, double alpha, const QColor &bottom)
{
    const auto mix = [alpha](float a, float b) { return a * alpha + b * (1 - alpha); };
    return QColor::fromRgbF(mix(top.redF(), bottom.redF()), mix(top.greenF(), bottom.greenF()), mix(top.blueF(), bottom.blueF()));
}

QString codepoint(uint value)
{
    if (value == 0 || value > 0x10FFFF || (value >= 0xD800 && value <= 0xDFFF)) return QString(QChar(QChar::ReplacementCharacter));
    const char32_t character = value;
    return QString::fromUcs4(&character, 1);
}

// "&amp;", "&#65;" or "&#x42;" as reported by md4c (MD_TEXT_ENTITY).
QString decodeEntity(const QByteArray &entity)
{
    if (entity.startsWith("&#")) {
        const bool hex = entity.size() > 3 && (entity[2] == 'x' || entity[2] == 'X');
        bool ok = false;
        const uint value = entity.mid(hex ? 3 : 2, entity.size() - (hex ? 4 : 3)).toUInt(&ok, hex ? 16 : 10);
        return ok ? codepoint(value) : QString(QChar(QChar::ReplacementCharacter));
    }
    const ENTITY *found = entity_lookup(entity.constData(), size_t(entity.size()));
    if (!found) return QString::fromUtf8(entity);
    return codepoint(found->codepoints[0]) + (found->codepoints[1] ? codepoint(found->codepoints[1]) : QString());
}

QString attribute(const MD_ATTRIBUTE &value)
{
    QString result;
    for (int i = 0; value.substr_offsets[i] < value.size; ++i) {
        const QByteArray part(value.text + value.substr_offsets[i], qsizetype(value.substr_offsets[i + 1] - value.substr_offsets[i]));
        if (value.substr_types[i] == MD_TEXT_NULLCHAR) result += QChar(QChar::ReplacementCharacter);
        else if (value.substr_types[i] == MD_TEXT_ENTITY) result += decodeEntity(part);
        else result += QString::fromUtf8(part);
    }
    return result;
}

int headingSize(unsigned level)
{
    static constexpr int sizes[] = {28, 21, 18, 14, 12, 12};
    return sizes[qBound(1u, level, 6u) - 1];
}

// Inline formatting is written on every text run: Qt drops inherited font
// properties on nested spans inside table cells.
struct Style {
    int size = BaseSize;
    bool bold = false, italic = false, strike = false;
    QColor color;
    QString family;
    QString css() const
    {
        QString result = QString("font-size:%1px;").arg(size);
        if (bold) result += "font-weight:600;";
        if (italic) result += "font-style:italic;";
        if (strike) result += "text-decoration:line-through;";
        if (color.isValid()) result += QString("color:%1;").arg(color.name());
        if (!family.isEmpty()) result += QString("font-family:'%1';").arg(family);
        return result;
    }
};

struct Frame {
    MD_BLOCKTYPE type = MD_BLOCK_DOC;
    QString html, raw;
    int blocks = 0;          // child blocks (or list rows) emitted so far
    bool afterRule = false;  // the previous child was a thematic break
    bool code = false;       // contains inline code
    bool ordered = false, loose = false, task = false, checked = false, head = false;
    unsigned number = 1, level = 0;
    int depth = 0, bodyRow = 0;
    MD_ALIGN align = MD_ALIGN_DEFAULT;
};

struct Link {
    qsizetype start = 0;
    QString label, destination;
    MarkdownLink target;
};

class Renderer {
public:
    Renderer(const MarkdownTheme &theme, const MarkdownLinkPolicy &links) : m_theme(theme), m_links(links)
    {
        m_frames.append(Frame{});
        Style base; base.color = theme.fg; m_styles.append(base);
    }
    QString html() const { return m_frames.first().html; }

    int enterBlock(MD_BLOCKTYPE type, void *detail)
    {
        if (type == MD_BLOCK_DOC) return 0;
        Frame frame; frame.type = type;
        switch (type) {
        case MD_BLOCK_H: {
            frame.level = static_cast<MD_BLOCK_H_DETAIL *>(detail)->level;
            const int size = headingSize(frame.level); const bool muted = frame.level == 6;
            pushStyle([&](Style &s) { s.size = size; s.bold = true; if (muted) s.color = m_theme.muted; });
            break;
        }
        default: break;
        }
        m_frames.append(frame);
        return 0;
    }

    int leaveBlock(MD_BLOCKTYPE type, void *)
    {
        if (type == MD_BLOCK_DOC) return 0;
        const Frame frame = m_frames.takeLast();
        switch (type) {
        case MD_BLOCK_P:
            append(QString("<p style=\"margin:%1px 0 0 0;line-height:%2px;\">").arg(gap(BlockGap)).arg(frame.code ? CodeLineHeight : LineHeight)
                   + frame.html + "</p>");
            break;
        case MD_BLOCK_H: {
            m_styles.removeLast();
            const int size = headingSize(frame.level), lineHeight = qRound(size * 1.25), top = gap(HeadingGap);
            if (frame.level <= 2)
                append(QString("<table width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:%1px;border-collapse:collapse;\">"
                               "<tr><td style=\"padding-bottom:%2px;border-bottom:1px solid %3;line-height:%4px;\">")
                           .arg(top).arg(qRound(size * 0.3)).arg(m_theme.borderMuted.name()).arg(lineHeight)
                       + frame.html + "</td></tr></table>");
            else append(QString("<p style=\"margin:%1px 0 0 0;line-height:%2px;\">").arg(top).arg(lineHeight) + frame.html + "</p>");
            break;
        }
        default:
            // Blocks without their own presentation keep their content.
            m_frames.last().html += frame.html;
            break;
        }
        return 0;
    }

    int enterSpan(MD_SPANTYPE type, void *detail)
    {
        switch (type) {
        case MD_SPAN_EM: pushStyle([](Style &s) { s.italic = true; }); break;
        case MD_SPAN_STRONG: pushStyle([](Style &s) { s.bold = true; }); break;
        case MD_SPAN_DEL: pushStyle([](Style &s) { s.strike = true; }); break;
        case MD_SPAN_CODE:
            m_inCode = true; m_frames.last().code = true;
            m_frames.last().html += QString("<span style=\"background-color:%1;\">").arg(MarkdownHtml::chipSentinel().name());
            break;
        case MD_SPAN_A: {
            Link link; link.start = m_frames.last().html.size();
            link.destination = attribute(static_cast<MD_SPAN_A_DETAIL *>(detail)->href);
            if (m_links) link.target = m_links(link.destination);
            const bool active = !link.target.href.isEmpty();
            m_linkStack.append(link);
            pushStyle([&](Style &s) { if (active) s.color = m_theme.accent; });
            break;
        }
        case MD_SPAN_IMG:
            if (!m_hidden) m_frames.last().html += run(QObject::tr("[Image attachment]"));
            ++m_hidden;
            break;
        default: break;
        }
        return 0;
    }

    int leaveSpan(MD_SPANTYPE type)
    {
        switch (type) {
        case MD_SPAN_EM: case MD_SPAN_STRONG: case MD_SPAN_DEL: m_styles.removeLast(); break;
        case MD_SPAN_CODE: m_inCode = false; m_frames.last().html += "</span>"; break;
        case MD_SPAN_A: {
            m_styles.removeLast();
            const Link link = m_linkStack.takeLast();
            if (link.target.href.isEmpty()) break;
            QString &html = m_frames.last().html;
            const QString &location = link.target.location;
            if (!location.isEmpty() && link.label != location && link.label != link.destination) {
                Style style = m_styles.last(); style.color = m_theme.accent;
                html += QString("<span style=\"%1\">").arg(style.css()) + (" (" + location + ")").toHtmlEscaped() + "</span>";
            }
            html.insert(link.start, QString("<a href=\"%1\" title=\"%2\" style=\"color:%3;text-decoration:none;\">")
                                        .arg(link.target.href.toHtmlEscaped(), link.target.tooltip.toHtmlEscaped(), m_theme.accent.name()));
            html += "</a>";
            break;
        }
        case MD_SPAN_IMG: --m_hidden; break;
        default: break;
        }
        return 0;
    }

    int text(MD_TEXTTYPE type, const MD_CHAR *data, MD_SIZE size)
    {
        if (m_hidden) return 0;
        Frame &frame = m_frames.last();
        if (type == MD_TEXT_BR) { frame.html += m_inCode ? QStringLiteral("&nbsp;") : QStringLiteral("<br>"); return 0; }
        QString value;
        if (type == MD_TEXT_NULLCHAR) value = QChar(QChar::ReplacementCharacter);
        else if (type == MD_TEXT_SOFTBR) value = QStringLiteral(" ");
        else if (type == MD_TEXT_ENTITY) value = decodeEntity(QByteArray(data, qsizetype(size)));
        else value = QString::fromUtf8(data, qsizetype(size));
        if (!m_linkStack.isEmpty()) m_linkStack.last().label += value;
        if (m_inCode) frame.html += value.toHtmlEscaped().replace(' ', QStringLiteral("&nbsp;"));
        else frame.html += run(value);
        return 0;
    }

private:
    void pushStyle(const std::function<void(Style &)> &change) { Style style = m_styles.last(); change(style); m_styles.append(style); }
    // The first block of a container has no gap; a block after a rule keeps the rule's gap.
    int gap(int wanted) const { const Frame &c = m_frames.last(); return c.blocks == 0 ? 0 : c.afterRule ? RuleGap : wanted; }
    void append(const QString &html, bool rule = false) { Frame &c = m_frames.last(); c.html += html; ++c.blocks; c.afterRule = rule; }
    QString run(const QString &value) const { return QString("<span style=\"%1\">").arg(m_styles.last().css()) + value.toHtmlEscaped() + "</span>"; }

    const MarkdownTheme &m_theme;
    const MarkdownLinkPolicy &m_links;
    QVector<Frame> m_frames;
    QVector<Style> m_styles;
    QVector<Link> m_linkStack;
    int m_hidden = 0;   // inside image alt text
    bool m_inCode = false;
};
}

MarkdownTheme MarkdownTheme::github(bool dark, const QColor &canvas, double scale)
{
    MarkdownTheme theme;
    theme.dark = dark; theme.scalePercent = qRound(scale * 100); theme.canvas = canvas;
    theme.fg = QColor(dark ? "#f0f6fc" : "#1f2328");
    theme.muted = QColor(dark ? "#9198a1" : "#59636e");
    theme.accent = QColor(dark ? "#4493f8" : "#0969da");
    theme.subtle = QColor(dark ? "#151b23" : "#f6f8fa");
    theme.border = QColor(dark ? "#3d444d" : "#d1d9e0");
    theme.borderMuted = over(theme.border, 0.7, canvas);
    theme.chip = QColor(dark ? "#656c76" : "#818b98"); theme.chip.setAlphaF(dark ? 0.2f : 0.12f);
    theme.monoFamily = MarkdownHtml::monospaceFamily();
    return theme;
}

QString MarkdownTheme::resource(const QString &name) const
{
    return QString("hgs-md:%1/%2/%3").arg(name, dark ? QStringLiteral("dark") : QStringLiteral("light"), QString::number(scalePercent));
}

QColor MarkdownHtml::chipSentinel() { return QColor(1, 2, 3); }

QString MarkdownHtml::monospaceFamily()
{
    static const QString family = [] {
        const QStringList installed = QFontDatabase::families();
        for (const char *candidate : {"ui-monospace", "SFMono-Regular", "SF Mono", "Menlo", "Consolas", "Liberation Mono"})
            if (installed.contains(QLatin1String(candidate), Qt::CaseInsensitive)) return QString::fromLatin1(candidate);
        return QFontDatabase::systemFont(QFontDatabase::FixedFont).family();
    }();
    return family;
}

QString MarkdownHtml::render(const QString &markdown, const MarkdownTheme &theme, const MarkdownLinkPolicy &links)
{
    Renderer renderer(theme, links);
    MD_PARSER parser{};
    parser.abi_version = 0;
    // GitHub semantics: `_x_` is emphasis, so MD_FLAG_UNDERLINE stays off. Raw HTML remains text.
    parser.flags = MD_DIALECT_GITHUB | MD_FLAG_NOHTML;
    parser.enter_block = [](MD_BLOCKTYPE type, void *detail, void *self) { return static_cast<Renderer *>(self)->enterBlock(type, detail); };
    parser.leave_block = [](MD_BLOCKTYPE type, void *detail, void *self) { return static_cast<Renderer *>(self)->leaveBlock(type, detail); };
    parser.enter_span = [](MD_SPANTYPE type, void *detail, void *self) { return static_cast<Renderer *>(self)->enterSpan(type, detail); };
    parser.leave_span = [](MD_SPANTYPE type, void *, void *self) { return static_cast<Renderer *>(self)->leaveSpan(type); };
    parser.text = [](MD_TEXTTYPE type, const MD_CHAR *text, MD_SIZE size, void *self) { return static_cast<Renderer *>(self)->text(type, text, size); };
    const QByteArray source = markdown.toUtf8();
    if (md_parse(source.constData(), MD_SIZE(source.size()), &parser, &renderer) != 0)
        return "<p>" + markdown.toHtmlEscaped() + "</p>";
    return renderer.html();
}
