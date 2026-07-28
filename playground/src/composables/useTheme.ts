import { ref, watchEffect } from "vue";

export type Theme = "light" | "dark";

export function useTheme() {
  const stored = window.localStorage.getItem("pg-playground-theme");
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const theme = ref<Theme>(
    stored === "light" || stored === "dark" ? stored : systemDark ? "dark" : "light",
  );

  watchEffect(() => {
    document.documentElement.dataset.theme = theme.value;
    document.documentElement.style.colorScheme = theme.value;
    window.localStorage.setItem("pg-playground-theme", theme.value);
  });

  function toggleTheme(): void {
    theme.value = theme.value === "dark" ? "light" : "dark";
  }

  return { theme, toggleTheme };
}
