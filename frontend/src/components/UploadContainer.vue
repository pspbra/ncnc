<script lang="ts">
import UploadItem from './UploadItem.vue';
import { NScrollbar } from 'naive-ui';

export default {
    props: {
        uploads: {
            type: Object as () => Array<{ id: string; name: string }>,
            required: true,
        },
    },
    components: {
        UploadItem,
        NScrollbar
    },
    mounted() {
        
    },
    unmounted() {
        // Stop the loop
        this.needRefresh = false;
    },
    data() {
        return {
            needRefresh: false,
        };
    },
    methods: {
        
    }
};

</script>

<template>
    <n-scrollbar style="max-height: 300px;">
        <div>
            <UploadItem v-if="uploads.length > 0" v-for="u in uploads" :key="u.id" :data="u" />
            <p v-else>正在进行的上传项会显示在这里</p>
        </div>
    </n-scrollbar>
</template>

<style scoped>

</style>
