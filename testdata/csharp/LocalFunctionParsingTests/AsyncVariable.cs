class C
{
    static void F(object async)
    {
        async.F();
        async->F();
        async = null;
        async += 1;
        async++;
        async[0] = null;
        async();
    }
    static void G()
    {
        async async;
        async.T t;
        async<object> u;
    }
    static void H()
    {
        async async() => 0;
        async F<T>() => 1;
        async async G<T>() { }
        async.T t() { }
        async<object> u(object o) => o;
    }
}