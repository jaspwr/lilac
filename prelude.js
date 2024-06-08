const __RELEASE = false;
let global = {};
let __internal_global = {};

let ID_COUNTER = 0;

const assert = (condition, message) => {
    if (__RELEASE === true) return;

    if (!condition) {
    	throw new Error(`[Lilac failed assert] ${message}`);
    }
}

const subscribe_fn = (subList) => (callback) => {
    const id = ID_COUNTER++;
    subList.set(id, callback);
    return () => subList.delete(id);
}

class State {
    __STATE = true;
    #value;
    #subscriptions = new Map();

    constructor(initialState) {
	this.#value = initialState;
    }

    run_subscriptions = () => {
	this.#subscriptions.forEach((sub) => sub(this.#value));
    }

    set = (setter) => {
	this.#value = setter(this.#value);
	this.run_subscriptions();
    }

    get = () => {
	return this.#value;
    }

    subscribe = subscribe_fn(this.#subscriptions);
}

const state = (initialState) => {
    const state = new State(initialState);
    return state; 
}

let __currently_rendering = null;
const __unmount_functions = {};

const unmount = (callback) => {
    if (__currently_rendering === null) return;

    if (__unmount_functions[__currently_rendering] === undefined) __unmount_functions[__currently_rendering] = [];
    __unmount_functions[__currently_rendering].push(callback);
};

const __run_unmounts = (id) => {
    if (__unmount_functions[id] !== undefined) {
	__unmount_functions[id].forEach((fn) => fn());
	delete __unmount_functions[id];
    }
}

let __list_id_counter = 0;

class LState extends State {
    __LSTATE = true;
    #addSubscriptions = new Map();
    #removeSubscriptions = new Map();
    #id = __list_id_counter++;

    #key_counter = 0;
    #keys = [];

    constructor(initialState) {
	assert(Array.isArray(initialState), "Initial value of lstate must be an array");

	super(initialState);
    }

    subscribeAdd = subscribe_fn(this.#addSubscriptions);
    subscribeRemove = subscribe_fn(this.#removeSubscriptions);

    length = () => this.get().length;

    new_key = (position) => {
	const key = `lstatekey_${this.#id}_${this.#key_counter++}`;
	this.#keys.splice(position, 0, key);
	return key;
    }

    handle_new_item = (item, position) => {
	const key = this.new_key(position);

	const outer_rendering = __currently_rendering;
	__currently_rendering = key;

	this.run_subscriptions();
	this.#addSubscriptions.forEach((sub) => sub(item, position));

	__currently_rendering = outer_rendering;
    }

    handle_remove = (position) => {
	const key = this.#keys[position];
	this.#keys.splice(position, 1);

	__run_unmounts(key);

	this.run_subscriptions();
	this.#removeSubscriptions.forEach((sub) => sub(position));
    }

    push = (item) => {
	this.get().push(item);
	const position = this.length() - 1;

	this.handle_new_item(item, position);
    }

    pop = () => {
	const position = this.length() - 1;
	
	this.handle_remove(position);

	return this.get().pop();
    }

    removeAt = (position) => {
	const _ = this.get().splice(position, 1);
	this.handle_remove(position);
    }

    insertAt = (position, item) => {
	this.get().splice(position, 0, item);

	this.handle_new_item(item, position);
    }

    findAndRemove = (predicate) => {
	const position = this.get().findIndex(predicate);
	if (position === -1) return;
	const _ = this.get().splice(position, 1);

	this.handle_remove(position);
    }

}

const lstate = (initialState) => {
    return new LState(initialState);
}

const __conditionals_previous_result = {};

