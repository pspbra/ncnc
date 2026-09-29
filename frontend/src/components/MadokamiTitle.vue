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
  <div class="wrapper">
    <img
      alt="NCNC logo"
      class="logo"
      src="@/assets/logo.png"
      width="125"
      height="125"
    />
  <div class="greetings">
    <h1 :style="{ color: primaryColor }">NCNC</h1>
    <h3 :style="{ color: `${primaryColor}71` }">
      火星群友们的番剧订阅与管理平台
    </h3>
  </div>
  </div>
</template>

<style scoped>

.logo {
  display: block;
  margin: 0 auto 2rem;
}

h1 {
  font-weight: 500;
  font-size: 2.6rem;
  position: relative;
  top: -10px;
}

h3 {
  font-size: 1.2rem;
}

.greetings {
  width: 100%;
}

.greetings h1,
.greetings h3 {
  text-align: center;
}

</style>
