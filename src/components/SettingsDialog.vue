<script setup lang="ts">
import { commands } from '../bindings.ts'
import { computed, ref } from 'vue'
import { path } from '@tauri-apps/api'
import { appDataDir } from '@tauri-apps/api/path'
import { useStore } from '../store.ts'

const store = useStore()

const showing = defineModel<boolean>('showing', { required: true })

const proxyHost = ref<string>(store.config?.proxyHost ?? '')

const disableProxyHostAndPort = computed(() => store.config?.proxyMode !== 'Custom')

async function showConfigInFileManager() {
  const configName = 'config.json'
  const configPath = await path.join(await appDataDir(), configName)
  const result = await commands.showPathInFileManager(configPath)
  if (result.status === 'error') {
    console.error(result.error)
  }
}
</script>

<template>
  <n-modal v-if="store.config !== undefined" v-model:show="showing">
    <n-dialog :showIcon="false" title="设置" content-style="" @close="showing = false">
      <div class="flex flex-col gap-row-2">
        <n-radio-group v-model:value="store.config.downloadFormat">
          下载格式：
          <n-tooltip placement="top" trigger="hover">
            <template #trigger>
              <n-radio value="Jpeg">jpg</n-radio>
            </template>
            1. 有损
            <span class="text-red">(肉眼看不出)</span>
            <br />
            2. 文件体积小
            <br />
            4. 宽高的上限为65534
            <span class="text-red">(某些条漫可能会超过这个上限导致报错)</span>
            <br />
            3. 编码速度最快
            <br />
          </n-tooltip>
          <n-tooltip placement="top" trigger="hover">
            <template #trigger>
              <n-radio value="Png">png</n-radio>
            </template>
            1. 无损
            <br />
            2. 文件体积大
            <span class="text-red">(约为jpg的5倍)</span>
            <br />
            3. 编码速度最慢
            <br />
          </n-tooltip>
          <n-tooltip placement="top" trigger="hover">
            <template #trigger>
              <n-radio value="Webp">webp</n-radio>
            </template>
            1. 无损
            <br />
            <span class="text-red">2. 这是jm图片原本的格式</span>
            <br />
            3. 文件体积大
            <span class="text-red">(约为jpg的4倍)</span>
            <br />
            4. 宽高的上限为16383
            <span class="text-red">(某些条漫可能会超过这个上限导致报错)</span>
            <br />
            5. 编码速度较慢
            <br />
          </n-tooltip>
        </n-radio-group>
        <n-radio-group v-model:value="store.config.proxyMode">
          代理类型：
          <n-radio value="System">系统代理</n-radio>
          <n-radio value="NoProxy">直连</n-radio>
          <n-radio value="Custom">自定义</n-radio>
        </n-radio-group>
        <n-input-group>
          <n-input-group-label size="small">http://</n-input-group-label>
          <n-input
            :disabled="disableProxyHostAndPort"
            v-model:value="proxyHost"
            size="small"
            placeholder=""
            @blur="store.config.proxyHost = proxyHost"
            @keydown.enter="store.config.proxyHost = proxyHost" />
          <n-input-group-label size="small">:</n-input-group-label>
          <n-input-number
            :disabled="disableProxyHostAndPort"
            v-model:value="store.config.proxyPort"
            size="small"
            placeholder=""
            :parse="(x: string) => parseInt(x)" />
        </n-input-group>
        <n-button class="ml-auto mt-2" size="small" @click="showConfigInFileManager">打开配置目录</n-button>
      </div>
    </n-dialog>
  </n-modal>
</template>
