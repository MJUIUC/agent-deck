import { useEffect, useState, useCallback } from "react";
import { setupApi } from "@/api/client";
import { SetupWizard } from "@/components/wizards/setup-wizard/SetupWizard";
import { DesktopLayout } from "@/layouts/DesktopLayout";
import { MobileLayout } from "@/layouts/MobileLayout";
import { useIsMobile } from "@/hooks/useIsMobile";
import styles from "@/App.module.css";

export function App() {
  // null = not yet checked, false = incomplete, true = complete
  const [setupComplete, setSetupComplete] = useState<boolean | null>(null);
  const isMobile = useIsMobile();

  useEffect(() => {
    setupApi
      .status()
      .then((res) => setSetupComplete(res.data.complete))
      .catch(() => setSetupComplete(true));
  }, []);

  const handleSetupComplete = useCallback(() => {
    setSetupComplete(true);
  }, []);

  // Setup status not yet known — show nothing to avoid flash
  if (setupComplete === null) {
    return <div className={styles.loadingScreen}>Loading…</div>;
  }

  // Setup is incomplete — render the wizard as the full page
  if (!setupComplete) {
    return <SetupWizard onComplete={handleSetupComplete} />;
  }

  return isMobile ? <MobileLayout /> : <DesktopLayout />;
}
