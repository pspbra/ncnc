import { defineStore } from "pinia";
import { type GlobalThemeOverrides } from "naive-ui";
import pinkTheme from "@/themes/pink";
import blueTheme from "@/themes/blue";


interface ThemeState {
  dark: boolean;
  theme: GlobalThemeOverrides;
  themeName: string;
}

function getTheme(key: string) {
    if (key === "pink") {
        return pinkTheme;
    } else if (key === "blue") {
        return blueTheme;
    } else {
        return pinkTheme;
    }
}

function loadThemeFromLocalStorage(): ThemeState {
    const themeName = localStorage.getItem("theme");
    const isDark = localStorage.getItem("theme-dark");
    const theme = {
        dark: false,
        theme: pinkTheme,
        themeName: "pink",
      };
    if (isDark) {
        theme.dark = isDark === "true";
    }
    if (themeName) {
        theme.themeName = themeName;
        theme.theme = getTheme(themeName);
    }
    return theme;
    }
    

export const useThemeStore = defineStore("theme", {
  state: (): ThemeState => {
    return loadThemeFromLocalStorage();
  },
  actions: {
    setDark(dark: boolean) {
        this.dark = dark;
        localStorage.setItem("theme-dark", dark.toString());
    },
    setTheme(themeName: string) {
        this.themeName = themeName;
        this.theme = getTheme(themeName);
        localStorage.setItem("theme", themeName);
    }
  }
});