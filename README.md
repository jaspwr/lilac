# This project is unfinished.

A simple intuitive meta-framework that is stripped away by the compiler
giving the user a fast and light application. The Svelte inspired syntax is explicit but terse and feels familiar to anyone who knows vanilla JavaScript. The framework can be built for web with JavaScript or a native OpenGL UI with Rust.

## Counter example
```
<script>
    const count = state(1);

    const addOne = () => count.set((c) => c + 1);
</script>

{$count}

<button onclick={addOne}>Add</button>
```
