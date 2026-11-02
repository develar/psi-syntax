
class C
{
    new C<>();
    new C<, >();
    C<C<>> a1;
    C<A<>> a1;
    object a1 = typeof(C<C<, >, int>);
    object a2 = Swap<>(1, 1);
}

class M<,> { }
