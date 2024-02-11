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

const state = (initialState) => {
    const state = {
	__STATE: true,
	value: initialState,
	subscriptions: new Map(),
    };

    state.subscribe = subscribe_fn(state.subscriptions); 

    state.set = (setter) => {
	state.value = setter(state.value);
	state.subscriptions.forEach((sub) => sub(state.value));
    };

    return state; 
}

const lstate = (initialState) => {
    assert(Array.isArray(initialState), "Initial value of lstate must be an array");

    const lstate = {
	__STATE: true,
	__LSTATE: true,
	value: initialState,
	subscriptions: new Map(),
	addSubscriptions: new Map(),
	removeSubscriptions: new Map(),
    };

    lstate.subscribe = subscribe_fn(lstate.subscriptions);
    lstate.subscribeAdd = subscribe_fn(lstate.addSubscriptions);
    lstate.subscribeRemove = subscribe_fn(lstate.removeSubscriptions);

    lstate.push = (item) => {
	lstate.value.push(item);
	const position = lstate.value.length - 1;
	lstate.subscriptions.forEach((sub) => sub(lstate.value));
	lstate.addSubscriptions.forEach((sub) => sub(item, position));
    };

    lstate.pop = () => {
	const _ = lstate.value.pop();
	const position = lstate.value.length;
	lstate.subscriptions.forEach((sub) => sub(lstate.value));
	lstate.removeSubscriptions.forEach((sub) => sub(position));
    };

    lstate.removeAt = (position) => {
	const _ = lstate.value.splice(position, 1);
	lstate.subscriptions.forEach((sub) => sub(lstate.value));
	lstate.removeSubscriptions.forEach((sub) => sub(position));
    };

    lstate.insertAt = (position, item) => {
	lstate.value.splice(position, 0, item);
	lstate.subscriptions.forEach((sub) => sub(lstate.value));
	lstate.addSubscriptions.forEach((sub) => sub(item, position));
    };

    lstate.findAndRemove = (predicate) => {
	const position = lstate.value.findIndex(predicate);
	if (position === -1) return;
	const _ = lstate.value.splice(position, 1);
	lstate.subscriptions.forEach((sub) => sub(lstate.value));
	lstate.removeSubscriptions.forEach((sub) => sub(position));
    };

    return lstate;
}
