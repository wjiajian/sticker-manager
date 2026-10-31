import { useEffect, useState } from "react";

export function usePurgeDays(purgeAt: number | null) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (purgeAt === null) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(timer);
  }, [purgeAt]);
  return purgeAt === null ? null : Math.max(0, Math.ceil((purgeAt - now) / 86_400_000));
}
