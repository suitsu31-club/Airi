// app/components/theme-switcher.tsx
"use client";

import {Button} from "@heroui/react";
import {useTheme} from "next-themes";
import {useEffect, useState} from "react";

export function ThemeSwitcher() {
  const [mounted, setMounted] = useState(false);
  const {resolvedTheme, setTheme, theme} = useTheme();

  useEffect(() => {
    setMounted(true);
  }, []);

  if (!mounted) return null;

  const activeTheme = theme === "system" ? resolvedTheme : theme;

  return (
    <div className="flex items-center gap-2">
      <Button
        variant={activeTheme === "light" ? "primary" : "secondary"}
        onPress={() => setTheme("light")}
      >
        Light
      </Button>
      <Button
        variant={activeTheme === "dark" ? "primary" : "secondary"}
        onPress={() => setTheme("dark")}
      >
        Dark
      </Button>
      <Button variant={theme === "system" ? "primary" : "secondary"} onPress={() => setTheme("system")}>
        System
      </Button>
    </div>
  );
}
