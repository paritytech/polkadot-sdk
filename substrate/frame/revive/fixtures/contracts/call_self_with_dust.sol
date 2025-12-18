contract CallSelfWithDust {
    function f() external payable {}

    function call() public payable {
        this.f{value: 10}();
    }
}
