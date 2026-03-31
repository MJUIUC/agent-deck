import { useEffect, useState } from "react";

/**
 * useIsMobile — returns true when the viewport is mobile-sized or the device
 * supports touch input.
 *
 * Criteria (either condition is sufficient):
 *   • window.innerWidth <= 768
 *   • Touch device: 'ontouchstart' in window || navigator.maxTouchPoints > 0
 *
 * The value updates reactively on every window resize event and the listener
 * is cleaned up when the component unmounts.
 */

function isMobileNow(): boolean {
  const narrowViewport = window.innerWidth <= 768;
  const touchDevice =
    "ontouchstart" in window || navigator.maxTouchPoints > 0;
  return narrowViewport || touchDevice;
}

export function useIsMobile(): boolean {
  const [isMobile, setIsMobile] = useState<boolean>(isMobileNow);

  useEffect(() => {
    function handleResize(): void {
      setIsMobile(isMobileNow());
    }

    window.addEventListener("resize", handleResize, { passive: true });

    // Sync once immediately in case the value changed between the initial
    // render and the effect running (e.g. SSR hydration or fast resize).
    handleResize();

    return () => {
      window.removeEventListener("resize", handleResize);
    };
  }, []);

  return isMobile;
}
