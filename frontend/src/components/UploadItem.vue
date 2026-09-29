<script lang="ts">
import { NEllipsis } from 'naive-ui';

export default {
  props: {
    data: {
      type: Object as () => { id: string; name: string },
      required: true,
    },
  },
  components: {
    NEllipsis
  },
};

</script>

<template>
<div class="upload-item">
  <n-ellipsis line-clamp="1">
    <p>正在上传 "{{ data.name }}"</p>
  </n-ellipsis>
</div>
</template>

<style scoped>
.upload-item {
  padding: 10px;
  border-bottom: 1px solid #f0f0f0;
  min-width: 300px;
}
</style>
