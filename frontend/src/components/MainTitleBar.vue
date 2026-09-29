<script lang="ts">
import { useThemeStore } from "@/stores/theme";

export default {
  setup() {
    const themeStore = useThemeStore();
    return { themeStore };
  },
  computed: {
    primaryColor() {
      if (this.themeStore.theme.common && this.themeStore.theme.common.primaryColor) {
        return this.themeStore.theme.common.primaryColor;
      }
      if (this.themeStore.themeName === 'blue') {
        return '#4dd2ff';
      } else if (this.themeStore.themeName === 'green') {
        return '#18a058';
      }
      return '#ff80aa';
    }
  }
}
</script>

<template>
  <div class="inline-container">
    <img
      alt="NCNC logo"
      class="logo"
      src="@/assets/logo.png"
      width="40"
      height="40"
    />
    <h2 :style="{ color: primaryColor }">NCNC</h2>
  </div>
</template>

<style scoped>
.inline-container {
    display: grid;
    grid-template-columns: auto 1fr;
    height: 50px;
    border-bottom: 1px solid #7877772a;
}

h2 {
    margin-left: 0.6rem;
    font-size: 1.7rem;
    font-weight: 500;
}
</style>