<template>
  <header class="page-head">
    <div class="grow">
      <h1><slot name="title" /></h1>
      <p><slot name="lead" /></p>
    </div>
    <div class="row"><slot name="actions" /></div>
  </header>
</template>
