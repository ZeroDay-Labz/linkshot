//! Automatic scrolling on Windows through UI Automation: the scrollable element under the region
//! (Explorer lists, browsers, Office documents, most WinUI/WPF/Win32 views) is scrolled with its
//! ScrollPattern instead of wheel events. Steps are exact percentages of the content and the end is
//! known, like the AT-SPI path on Linux (`a11y_scroll.rs`), whose interface this mirrors.

use anyhow::{bail, Context, Result};
use windows::Win32::Foundation::POINT;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, IUIAutomationScrollPattern, UIA_ScrollPatternId, UIA_ScrollPatternNoScroll};

pub struct A11yScroller {
    pattern: IUIAutomationScrollPattern,
    horizontal: bool,
    /// A first guess for one step, in percent of the content (calibrated by the caller).
    pub initial_step: f64,
}

impl A11yScroller {
    /// Finds the nearest scrollable ancestor of the element at `center` (screen coordinates).
    pub fn find(center: (f64, f64), horizontal: bool) -> Result<A11yScroller> {
        unsafe {
            // The scrolling thread is ours: a multithreaded apartment is fine (S_FALSE = already set).
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).context("UI Automation")?;
            let mut el = uia.ElementFromPoint(POINT { x: center.0.round() as i32, y: center.1.round() as i32 }).context("no element under the region")?;
            let walker = uia.ControlViewWalker()?;
            for _ in 0..24 {
                if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId) {
                    let scrollable = if horizontal { p.CurrentHorizontallyScrollable() } else { p.CurrentVerticallyScrollable() };
                    if scrollable.map(|b| b.as_bool()).unwrap_or(false) {
                        let view = if horizontal { p.CurrentHorizontalViewSize() } else { p.CurrentVerticalViewSize() }.unwrap_or(20.0);
                        tracing::info!("UI Automation scrolling: view shows {view:.0} % of the content");
                        return Ok(A11yScroller { pattern: p, horizontal, initial_step: (view * 0.3).clamp(0.5, 50.0) });
                    }
                }
                match walker.GetParentElement(&el) { Ok(parent) => el = parent, Err(_) => break }
            }
            bail!("no scrollable element under the region")
        }
    }

    fn percent(&self) -> Result<f64> {
        unsafe { Ok(if self.horizontal { self.pattern.CurrentHorizontalScrollPercent()? } else { self.pattern.CurrentVerticalScrollPercent()? }) }
    }

    fn set(&self, v: f64) -> Result<()> {
        unsafe {
            let (h, vv) = if self.horizontal { (v, UIA_ScrollPatternNoScroll) } else { (UIA_ScrollPatternNoScroll, v) };
            self.pattern.SetScrollPercent(h, vv).context("set scroll position")
        }
    }

    pub fn to_start(&self) -> Result<()> { self.set(0.0) }

    /// Moves by `delta` percent (negative = back); returns how far it actually moved (0 at the end).
    pub fn scroll_by(&self, delta: f64) -> Result<f64> {
        let cur = self.percent()?;
        if cur < 0.0 { bail!("the view is not scrollable any more"); }
        let target = (cur + delta).clamp(0.0, 100.0);
        if (target - cur).abs() < 0.001 { return Ok(0.0); }
        self.set(target)?;
        Ok(target - cur)
    }

    pub fn range(&self) -> f64 { 100.0 }
}
