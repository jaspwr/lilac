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

class LState extends State {
    __LSTATE = true;
    #addSubscriptions = new Map();
    #removeSubscriptions = new Map();

    constructor(initialState) {
	assert(Array.isArray(initialState), "Initial value of lstate must be an array");

	super(initialState);
    }

    subscribeAdd = subscribe_fn(this.#addSubscriptions);
    subscribeRemove = subscribe_fn(this.#removeSubscriptions);

    length = () => this.get().length;

    push = (item) => {
	this.get().push(item);
	this.run_subscriptions();
	const position = this.length() - 1;
	this.#addSubscriptions.forEach((sub) => sub(item, position));
    }

    pop = () => {
	return this.get().pop();
    }

    removeAt = (position) => {
	const _ = this.get().splice(position, 1);
	this.run_subscriptions();
	this.#removeSubscriptions.forEach((sub) => sub(position));
    }

    insertAt = (position, item) => {
	this.get().splice(position, 0, item);
	this.run_subscriptions();
	this.#addSubscriptions.forEach((sub) => sub(item, position));
    }

    findAndRemove = (predicate) => {
	const position = this.get().findIndex(predicate);
	if (position === -1) return;
	const _ = this.get().splice(position, 1);
	this.run_subscriptions();
	this.#removeSubscriptions.forEach((sub) => sub(position));
    }

}

const lstate = (initialState) => {
    return new LState(initialState);
}
