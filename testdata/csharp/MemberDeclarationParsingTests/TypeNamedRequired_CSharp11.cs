class required { }

class C
{
    required _required;
    required[] _array;
    required* _ptr;
    required? _nullable;
    delegate*<required, required> _funcPtr;
    (required, required) _tuple;
}