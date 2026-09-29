<script lang="ts">
import { RouterView } from "vue-router";
import MessageContainer from "./MessageContainer.vue";
import { NMessageProvider } from "naive-ui";


export default {
  components: {
    RouterView,
    MessageContainer,
    NMessageProvider
  },
  setup() {
    return {
    }
  }
}

</script>

<template>
  <n-message-provider>
  <MessageContainer />
  <header>
    <RouterView />
  </header>
  </n-message-provider>
</template>

<style scoped>
header {
  line-height: 1.5;
}

</style>
