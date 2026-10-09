package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class TerminalAnsiTest {
    private val esc = "\u001b"
    @Test fun basicBrightAndResetColorsMatchMuxPod() {
        val screen = TerminalAnsi().parse("${esc}[31mred${esc}[92mgreen${esc}[0mplain")
        assertEquals("redgreenplain",screen.text)
        assertEquals(0xFFCD3131.toInt(),screen.spans[0].style.foreground)
        assertEquals(0xFF23D18B.toInt(),screen.spans[1].style.foreground)
        assertEquals(TerminalStyle(),screen.spans[2].style)
    }
    @Test fun extendedColorsAndIndependentAttributes() {
        val screen = TerminalAnsi().parse("${esc}[1;2;3;4;7;9;38;5;196;48;2;1;2;3ma${esc}[22;23;24;27;29;39;49mb")
        assertEquals(TerminalStyle(0xFFFF0000.toInt(),0xFF010203.toInt(),true,true,true,true,true,true),screen.spans[0].style)
        assertEquals(TerminalStyle(),screen.spans[1].style)
        assertEquals(0xFF080808.toInt(),TerminalAnsi.palette(232))
        assertEquals(0xFFEEEEEE.toInt(),TerminalAnsi.palette(255))
    }
    @Test fun styleContinuesAcrossLinesAndCachedReusePreservesIt() {
        val parser = TerminalAnsi()
        val first = parser.parse("${esc}[32mfirst\nsecond")
        val next = parser.parse("${esc}[31mfirst\nsecond")
        assertEquals(0xFF0DBC79.toInt(),first.spans.last().style.foreground)
        assertEquals(0xFFCD3131.toInt(),next.spans.last().style.foreground)
        assertEquals(next,parser.parse("${esc}[31mfirst\nsecond"))
    }
    @Test fun oscClipboardTitleAndLinksNeverReachVisibleText() {
        val screen = TerminalAnsi().parse("A${esc}]52;c;secret\u0007B${esc}]0;title${esc}\\C${esc}]8;;https://example.invalid\u0007label${esc}]8;;${esc}\\D")
        assertEquals("ABClabelD",screen.text)
        assertFalse(screen.text.contains("secret"))
    }
    @Test fun c1ControlsAndOtherEscapePayloadsAreDiscarded() {
        assertEquals("abcd",TerminalAnsi().parse("a\u009d52;c;private\u009cb${esc}Pprivate${esc}\\c${esc}[2Jd").text)
        assertEquals("safe",TerminalAnsi().parse("safe${esc}]52;unterminated").text)
    }
    @Test fun unicodeTabsUseCellsWithoutSplittingSurrogates() {
        assertEquals("界      x",TerminalAnsi().parse("界\tx").text)
        assertEquals("e\u0301       x",TerminalAnsi().parse("e\u0301\tx").text)
        assertEquals("😀      x",TerminalAnsi().parse("😀\tx").text)
        assertEquals(0..1,TerminalAnsi.cursorRange("😀x",1,0))
        assertEquals(2..2,TerminalAnsi.cursorRange("😀x",2,0))
        // Blank end-of-line is rendered using the native cell coordinates, not a character range.
        assertNull(TerminalAnsi.cursorRange("x",1,0))
        assertNull(TerminalAnsi.cursorRange("",0,0))
    }
    @Test fun emptyAndMalformedExtendedSequencesDoNotInventColors() {
        assertEquals(TerminalStyle(),TerminalAnsi().parse("${esc}[38;2;999;0;0mx").spans.single().style)
        assertEquals("text",TerminalAnsi().parse("text${esc}[").text)
        assertTrue(TerminalAnsi().parse("x".repeat(140000)).truncated)
        assertTrue(TerminalAnsi().parse("\n".repeat(1200)).truncated)
    }
}
