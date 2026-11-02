public struct closed { public int item; }

public unsafe class C
{
    public closed _closed;
    public closed[] _array;
    public closed* _ptr;
    public closed? _nullable;
    public delegate*<closed, closed> _funcPtr;
    public (closed, closed) _tuple;
}