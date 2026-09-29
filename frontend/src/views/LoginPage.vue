<script setup lang="ts">
import MadokamiTitle from "@/components/MadokamiTitle.vue";
import LoginBox from "@/components/LoginBox.vue";
</script>

<template>
  <div class="container">
    <header>
    <div class="wrapper">
      <MadokamiTitle />
    </div>
    <LoginBox />
  </header>
  </div>
  
  
</template>

<style scoped>
header {
  line-height: 1.5;
  width: 100%;
  opacity: 0;
  animation: fadeUp 2s ease forwards;
  max-width: 1000px;
}

.container {
  height: 100vh;
  display: grid;
  place-items: center;
}

@media (min-width: 1024px) {
  header {
    display: grid;
    grid-template-columns: 1fr 1fr;
    padding: 0 2rem;
    align-items: center;
  }
}

@keyframes fadeUp {
  from {
    /* 动画开始状态：完全透明且向下30px */
    opacity: 0;
    transform: translateY(30px);
  }
  to {
    /* 动画结束状态：完全不透明且在原位 */
    opacity: 1;
    transform: translateY(0);
  }
}

</style>