/*
 * Copyright 2025 mox
 * Licensed under the Apache License, Version 2.0 (see assets/licenses/mux-pod.txt).
 * Adapted from MuxPod lib/services/terminal/ansi_parser.dart, commit
 * c33d0fdeb66ca7bbdf4745203578088961982c13.
 * Modified for Zerus: Kotlin model, bounded parsing, control suppression and cursor mapping.
 */
package app.zerus.mobile

data class TerminalStyle(val foreground: Int? = null, val background: Int? = null,
    val bold: Boolean = false, val dim: Boolean = false, val italic: Boolean = false,
    val underline: Boolean = false, val strike: Boolean = false, val inverse: Boolean = false)
data class TerminalSpan(val start: Int, val end: Int, val style: TerminalStyle)
data class TerminalScreen(val text: String, val spans: List<TerminalSpan>, val truncated: Boolean)
private data class TerminalLine(val text: String, val spans: List<TerminalSpan>, val endStyle: TerminalStyle)

/** Parses captured text, not a terminal emulator. Escape sequences never execute actions. */
class TerminalAnsi {
    private var previous = emptyMap<Pair<TerminalStyle,String>,TerminalLine>()

    fun parse(input: String): TerminalScreen {
        val bounded = input.take(128 * 1024)
        val clean = StringBuilder()
        var i = 0
        // Keep only bounded SGR; remove OSC (including hyperlinks/title/clipboard), DCS and CSI controls.
        while (i < bounded.length) {
            val c = bounded[i++]
            if (c == '\u001b' || c in listOf('\u009b','\u009d','\u0090','\u009e','\u009f')) {
                if (c == '\u001b' && i == bounded.length) break
                val control = if(c == '\u001b') bounded[i++] else when(c) {
                    '\u009b' -> '['; '\u009d' -> ']'; '\u0090' -> 'P'; '\u009e' -> '^'; else -> '_'
                }
                when (control) {
                    '[' -> {
                        val start = i
                        while (i < bounded.length && bounded[i] !in '@'..'~') i++
                        if (i < bounded.length) {
                            val final = bounded[i++]
                            val body = bounded.substring(start,i - 1)
                            if (final == 'm' && body.length <= 256 && body.all { it.isDigit() || it == ';' }) clean.append("\u001b[").append(body).append('m')
                        }
                    }
                    ']', 'P', '^', '_' -> {
                        while (i < bounded.length) {
                            if (bounded[i] == '\u0007' || bounded[i] == '\u009c') { i++; break }
                            if (bounded[i] == '\u001b' && bounded.getOrNull(i + 1) == '\\') { i += 2; break }
                            i++
                        }
                    }
                    '(', ')' -> if (i < bounded.length) i++ // Character set designation.
                    else -> Unit
                }
            } else if (c == '\n' || c == '\t' || c >= ' ' && c !in '\u007f'..'\u009f') clean.append(c)
        }
        val next = linkedMapOf<Pair<TerminalStyle,String>,TerminalLine>()
        val text = StringBuilder()
        val spans = mutableListOf<TerminalSpan>()
        var style = TerminalStyle()
        var truncated = input.length > bounded.length
        val lines = clean.toString().split('\n')
        for ((index, raw) in lines.take(1000).withIndex()) {
            val key = style to raw
            val line = next[key] ?: previous[key] ?: line(raw,style)
            next[key] = line
            if (index > 0) text.append('\n')
            val offset = text.length
            text.append(line.text)
            if (spans.size + line.spans.size > 8192) { truncated = true; break }
            spans += line.spans.map { it.copy(start = it.start + offset,end = it.end + offset) }
            style = line.endStyle
        }
        previous = next
        return TerminalScreen(text.toString(),spans,truncated || lines.size > 1000)
    }

    private fun line(raw: String, initial: TerminalStyle): TerminalLine {
        var style = initial
        val text = StringBuilder()
        val spans = mutableListOf<TerminalSpan>()
        var start = 0
        var i = 0
        var column = 0
        fun flush() { if(text.length > start) spans += TerminalSpan(start,text.length,style); start = text.length }
        while (i < raw.length) {
            if(raw[i] == '\u001b') {
                flush()
                val end = raw.indexOf('m',i + 2)
                style = sgr(raw.substring(i + 2,end),style)
                i = end + 1
            } else if (raw[i] == '\t') {
                val spaces = 8 - column % 8
                text.append(" ".repeat(spaces)); column += spaces; i++
            } else {
                val cp = Character.codePointAt(raw,i)
                text.appendCodePoint(cp); column += cellWidth(cp); i += Character.charCount(cp)
            }
        }
        flush()
        return TerminalLine(text.toString(),spans,style)
    }

    private fun sgr(body: String, initial: TerminalStyle): TerminalStyle {
        if(body.isEmpty()) return TerminalStyle()
        val codes = body.split(';').map { if(it.isEmpty()) 0 else it.toIntOrNull() ?: -1 }
        var style = initial
        var i = 0
        while(i < codes.size) {
            when(val code = codes[i++]) {
                0 -> style = TerminalStyle()
                1 -> style = style.copy(bold = true)
                2 -> style = style.copy(dim = true)
                3 -> style = style.copy(italic = true)
                4 -> style = style.copy(underline = true)
                7 -> style = style.copy(inverse = true)
                9 -> style = style.copy(strike = true)
                21,22 -> style = style.copy(bold = false,dim = false)
                23 -> style = style.copy(italic = false)
                24 -> style = style.copy(underline = false)
                27 -> style = style.copy(inverse = false)
                29 -> style = style.copy(strike = false)
                in 30..37 -> style = style.copy(foreground = palette(code - 30))
                in 40..47 -> style = style.copy(background = palette(code - 40))
                in 90..97 -> style = style.copy(foreground = palette(code - 90 + 8))
                in 100..107 -> style = style.copy(background = palette(code - 100 + 8))
                39 -> style = style.copy(foreground = null)
                49 -> style = style.copy(background = null)
                38,48 -> {
                    val color = when(codes.getOrNull(i++)) {
                        5 -> codes.getOrNull(i++)?.takeIf { it in 0..255 }?.let(::palette)
                        2 -> if(i + 2 < codes.size) {
                            val rgb = codes.subList(i,i + 3); i += 3
                            if(rgb.all { it in 0..255 }) argb(rgb[0],rgb[1],rgb[2]) else null
                        } else { i = codes.size; null }
                        else -> null
                    }
                    if(color != null) style = if(code == 38) style.copy(foreground = color) else style.copy(background = color)
                }
            }
        }
        return style
    }

    companion object {
        private val basic = listOf(0xFF000000,0xFFCD3131,0xFF0DBC79,0xFFE5E510,0xFF2472C8,0xFFBC3FBC,0xFF11A8CD,0xFFE5E5E5,
            0xFF666666,0xFFF14C4C,0xFF23D18B,0xFFF5F543,0xFF3B8EEA,0xFFD670D6,0xFF29B8DB,0xFFFFFFFF).map { it.toInt() }
        private fun argb(r: Int,g: Int,b: Int) = (0xff shl 24) or (r shl 16) or (g shl 8) or b
        fun palette(index: Int): Int {
            require(index in 0..255)
            if(index < 16) return basic[index]
            if(index < 232) {
                val n = index - 16
                fun level(value: Int) = if(value == 0) 0 else value * 40 + 55
                return argb(level(n / 36 % 6),level(n / 6 % 6),level(n % 6))
            }
            val gray = (index - 232) * 10 + 8
            return argb(gray,gray,gray)
        }
        fun cellWidth(cp: Int): Int = if(Character.getType(cp) in listOf(Character.NON_SPACING_MARK.toInt(),Character.ENCLOSING_MARK.toInt(),Character.COMBINING_SPACING_MARK.toInt())) 0
            else if(cp in 0x1100..0x115f || cp in 0x2e80..0xa4cf || cp in 0xac00..0xd7a3 || cp in 0xf900..0xfaff || cp in 0xfe10..0xfe6f || cp in 0xff00..0xff60 || cp in 0x1f300..0x1faff || cp in 0x20000..0x3ffff) 2 else 1
        /** Maps the native cell position to a UTF-16 range without splitting surrogate pairs. */
        fun cursorRange(text: String,x: Int,y: Int): IntRange? {
            if(x !in 0..999 || y !in 0..999) return null
            var start = 0
            repeat(y) { val newline = text.indexOf('\n',start); if(newline < 0) return null; start = newline + 1 }
            val end = text.indexOf('\n',start).let { if(it < 0) text.length else it }
            var cells = 0; var i = start
            while(i < end) {
                val cp = Character.codePointAt(text,i)
                val units = Character.charCount(cp)
                val width = cellWidth(cp)
                if(width > 0 && x in cells until cells + width) return i until i + units
                cells += width; i += units
            }
            return null
        }
    }
}
