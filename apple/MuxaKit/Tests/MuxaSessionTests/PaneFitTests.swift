import Testing

@testable import MuxaSession

struct PaneFitTests {
    let floor = PaneFit.minimumFontSize

    @Test func aTallViewShowsTheWholePaneWhileItIsReadable() {
        #expect(PaneFit.automatic(wide: false, wholeFont: 12, widthFont: 14) == .whole)
    }

    @Test func aTallViewFitsTheWidthWhenTheWholePaneWouldBeTooSmall() {
        // iPad mini in portrait with a tall pane.
        #expect(PaneFit.automatic(wide: false, wholeFont: 7, widthFont: 10) == .width)
    }

    @Test func aWideViewFitsTheWidth() {
        #expect(PaneFit.automatic(wide: true, wholeFont: 20, widthFont: 14) == .width)
    }

    @Test func aPhoneGetsTheReadableViewportWhenEvenTheWidthIsTooSmall() {
        // 100 columns on a 390 pt wide phone.
        #expect(PaneFit.automatic(wide: false, wholeFont: 5, widthFont: 6.5) == .readable)
        #expect(PaneFit.automatic(wide: true, wholeFont: 3, widthFont: 6.5) == .readable)
    }

    @Test func readableNeverGoesBelowTheFloor() {
        #expect(PaneFit.readable.fontSize(wholeFont: 4, widthFont: 6) == floor)
        #expect(PaneFit.readable.fontSize(wholeFont: 4, widthFont: 13) == 13)
        #expect(PaneFit.whole.fontSize(wholeFont: 4, widthFont: 6) == 4)
        #expect(PaneFit.width.fontSize(wholeFont: 4, widthFont: 6) == 6)
    }

    @Test func aPinchInShowsTheWholePaneAndOutMakesItReadable() {
        #expect(PaneFit.pinched(scale: 0.5) == .whole)
        #expect(PaneFit.pinched(scale: 2) == .readable)
        #expect(PaneFit.pinched(scale: 1.05) == nil)
    }
}
