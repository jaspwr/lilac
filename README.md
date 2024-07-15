# This project is unfinished.

<p align="center">
    <img src="assets/lilac.svg" width="120px" alt="Lilac logo">
</p>

A simple intuitive meta-framework that is stripped away by the compiler
giving the user a fast and light application. The Svelte inspired syntax is explicit but terse and feels familiar to anyone who knows vanilla JavaScript.

## Counter example
```
<script>
    const count = state(1);

    const addOne = () => count.set((c) => c + 1);
</script>

{$count}

<button onclick={addOne}>Add</button>
```
