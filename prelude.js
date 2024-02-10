let global = {};
let __internal_global = {};

let ID_COUNTER = 0;

const state = (initialState) => {
    const stackTraceError = new Error();
    const callStack = stackTraceError.stack.split('\n').slice(2);

    console.log('Calling stack:', callStack);


    const state = {
	__STATE: true,
	value: initialState,
	subscriptions: new Map(),
    };

    const setState = (setter) => {
	state.value = setter(state.value);
	state.subscriptions.forEach((sub) => sub());
    }

    state.set = setState;

    const subscribe = (callback) => {
	const id = ID_COUNTER++;
	state.subscriptions.set(id, callback);
	return () => state.subscriptions.delete(id);
    }

    state.subscribe = subscribe;

    return state; 
}
