import { useEffect } from "react";
import { LoaderCircle } from "lucide-react";
import { store, useStore } from "../state/store";
import { useMotionPreference } from "../state/useMotion";
import { Sidebar } from "./Sidebar";
import { Header } from "./Header";
import { Button, Toasts } from "../components/ui";
import { HomePage } from "../pages/home/HomePage";
import { HistoryPage } from "../pages/history/HistoryPage";
import { SettingsPage } from "../pages/settings/SettingsPage";
import { LogoMark } from "../components/Logo";

export function App() {
  const ready = useStore((s) => s.snapshot !== null);
  const bootError = useStore((s) => s.bootError);
  const page = useStore((s) => s.page);
  const brand = useStore((s) => s.snapshot?.brand.displayName ?? "Sayvane");
  const os = useStore((s) => s.snapshot?.platform.os ?? "other");
  useMotionPreference();

  useEffect(() => {
    document.title = brand;
  }, [brand]);
  useEffect(() => {
    document.documentElement.dataset.os = os;
  }, [os]);

  if (!ready) {
    return (
      <div className="boot" role="status" aria-live="polite">
        <LogoMark size={44} />
        {bootError ? (
          <>
            <p className="boot-title">Couldn't connect to the app core</p>
            <p className="boot-text">{bootError}</p>
            <Button
              variant="secondary"
              onClick={() => {
                store.set({ bootError: null });
                location.reload();
              }}
            >
              Try again
            </Button>
          </>
        ) : (
          <p className="boot-text">
            <LoaderCircle size={14} className="spin" aria-hidden /> Starting…
          </p>
        )}
      </div>
    );
  }

  const title = page === "home" ? "Home" : page === "history" ? "History" : "Settings";
  return (
    <div className="shell">
      <Sidebar />
      <main className="main" aria-labelledby="page-title">
        <Header title={title} />
        <div className="page-scroll" key={page}>
          {page === "home" && <HomePage />}
          {page === "history" && <HistoryPage />}
          {page === "settings" && <SettingsPage />}
        </div>
      </main>
      <Toasts />
    </div>
  );
}
