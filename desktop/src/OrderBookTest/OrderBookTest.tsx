
import React, { Component } from 'react';
import './OrderBookTest.css'


interface Order {
  id: string;
  price: number;
  quantity: number;
  total: number;
  timestamp: number;
  side: 'bid' | 'ask';
  userId?: string;
}


interface Trade {
  id: string;
  price: number;
  quantity: number;
  timestamp: string;
  side: 'buy' | 'sell';
  bidOrderId?: string;
  askOrderId?: string;
}


interface TestOrder {
  id: string;
  type: 'limit' | 'market' | 'cancel' | 'modify';
  side: 'bid' | 'ask';
  price?: number;
  quantity: number;
  originalOrderId?: string;
}


interface OrderBook {
  bids: Order[];
  asks: Order[];
}


interface Range {
  min: number;
  max: number;
}


interface TestParams {
  orderCount: number;
  priceRange: Range;
  quantityRange: Range;
  updateFrequency: number;
  volatility: number;
  marketOrderRatio: number;
  largeOrderRatio: number;
  cancelOrderRatio: number;
  testUserCount: number;
  allowSelfTrade: boolean;
}


interface OrderBookTestState {
  orderBook: OrderBook;
  testParams: TestParams;
  isTesting: boolean;
  testLogs: string[];
  currentPrice: number;
  trades: Trade[];
  activeTab: string;
  performanceMetrics: {
    orderProcessTime: number;
    matchEfficiency: number;
    throughput: number;
    memoryUsage: number;
    totalOrders: number;
    totalTrades: number;
  };
  testScenarios: {
    [key: string]: TestOrder[];
  };
  isDarkTheme: boolean;
  isEnglish: boolean;
}

class OrderBookTest extends Component<{}, OrderBookTestState> {
  testInterval?: NodeJS.Timeout;
  performanceTestInterval?: NodeJS.Timeout;
  testStartTime: number = 0;
  ordersProcessed: number = 0;


  private orderIdCounter: number = 0;
  private tradeIdCounter: number = 0;


  private asksBarsContainerRef = React.createRef<HTMLDivElement>();
  private ladderAsksContainerRef = React.createRef<HTMLDivElement>();

  constructor(props: {}) {
    super(props);
    this.state = {
      orderBook: {
        bids: [],
        asks: []
      },
      testParams: {
        orderCount: 100,
        priceRange: { min: 490, max: 510 },
        quantityRange: { min: 100, max: 300 },
        updateFrequency: 1000,
        volatility: 0.1,
        marketOrderRatio: 0.2,
        largeOrderRatio: 0.1,
        cancelOrderRatio: 0.15,
        testUserCount: 10,
        allowSelfTrade: false
      },
      isTesting: false,
      testLogs: [],
      currentPrice: 500.0,
      trades: [],
      activeTab: 'basic',
      performanceMetrics: {
        orderProcessTime: 0,
        matchEfficiency: 100,
        throughput: 0,
        memoryUsage: 0,
        totalOrders: 0,
        totalTrades: 0
      },
      testScenarios: {},
      isDarkTheme: true,
      isEnglish: true,
    };

    this.initializeTestScenarios();
  }


  scrollToBottom = () => {

    if (this.asksBarsContainerRef.current) {
      const container = this.asksBarsContainerRef.current;
      container.scrollTop = container.scrollHeight;
    }


    if (this.ladderAsksContainerRef.current) {
      const container = this.ladderAsksContainerRef.current;
      container.scrollTop = container.scrollHeight;
    }
  };


  componentDidUpdate(prevProps: {}, prevState: OrderBookTestState) {

    if (prevState.orderBook.asks.length !== this.state.orderBook.asks.length) {
      setTimeout(() => {
        this.scrollToBottom();
      }, 0);
    }
  }


  initializeTestScenarios = () => {
    const scenarios: { [key: string]: TestOrder[] } = {
      basicFunctionality: [
        { id: '1', type: 'limit', side: 'bid', price: 495, quantity: 100 },
        { id: '2', type: 'limit', side: 'ask', price: 505, quantity: 100 },
        { id: '3', type: 'limit', side: 'bid', price: 496, quantity: 200 },
        { id: '4', type: 'cancel', side: 'bid', quantity: 0, originalOrderId: '1' },
        { id: '5', type: 'modify', side: 'bid', price: 497, quantity: 150, originalOrderId: '3' }
      ],
      matchingLogic: [
        { id: '1', type: 'limit', side: 'ask', price: 500, quantity: 100 },
        { id: '2', type: 'market', side: 'bid', quantity: 50 },
        { id: '3', type: 'limit', side: 'bid', price: 501, quantity: 100 },
        { id: '4', type: 'limit', side: 'ask', price: 499, quantity: 200 }
      ],
      sortingRules: [
        { id: '1', type: 'limit', side: 'bid', price: 495, quantity: 100 },
        { id: '2', type: 'limit', side: 'bid', price: 495, quantity: 200 },
        { id: '3', type: 'limit', side: 'ask', price: 505, quantity: 100 },
        { id: '4', type: 'limit', side: 'ask', price: 505, quantity: 200 }
      ],
      boundaryConditions: [
        { id: '1', type: 'limit', side: 'bid', price: -100, quantity: 100 },
        { id: '2', type: 'limit', side: 'ask', price: 1000000, quantity: 100 },
        { id: '3', type: 'limit', side: 'bid', price: 500, quantity: 0 },
        { id: '4', type: 'limit', side: 'ask', price: 500, quantity: 1000000000 }
      ]
    };

    this.setState({ testScenarios: scenarios });
  };


  generateOrderId = (): string => {
    return `order_${++this.orderIdCounter}_${Date.now()}`;
  };


  generateUserId = (): string => {
    return `user_${Math.floor(Math.random() * this.state.testParams.testUserCount) + 1}`;
  };


  addOrderToBook = (order: Order) => {
    const { orderBook } = this.state;
    const sideArray = order.side === 'bid' ? [...orderBook.bids] : [...orderBook.asks];


    const newArray = [...sideArray, order];


    if (order.side === 'bid') {
      newArray.sort((a, b) => b.price - a.price);
    } else {
      newArray.sort((a, b) => a.price - b.price);
    }


    let total = 0;
    const withTotals = newArray.map(item => {
      total += item.quantity;
      return { ...item, total };
    });

    if (order.side === 'bid') {
      this.setState({
        orderBook: { ...orderBook, bids: withTotals }
      });
    } else {
      this.setState({
        orderBook: { ...orderBook, asks: withTotals }
      });
    }

    this.setState(prevState => ({
      performanceMetrics: {
        ...prevState.performanceMetrics,
        totalOrders: prevState.performanceMetrics.totalOrders + 1
      }
    }));
  };


  removeOrderFromBook = (orderId: string, side: 'bid' | 'ask') => {
    const { orderBook } = this.state;
    const sideArray = side === 'bid' ? orderBook.bids : orderBook.asks;

    const newArray = sideArray.filter(order => order.id !== orderId);


    let total = 0;
    const withTotals = newArray.map(item => {
      total += item.quantity;
      return { ...item, total };
    });

    if (side === 'bid') {
      this.setState({
        orderBook: { ...orderBook, bids: withTotals }
      });
    } else {
      this.setState({
        orderBook: { ...orderBook, asks: withTotals }
      });
    }
  };


  modifyOrderInBook = (orderId: string, side: 'bid' | 'ask', newQuantity: number, newPrice?: number) => {
    this.removeOrderFromBook(orderId, side);

    if (newPrice !== undefined && newQuantity > 0) {
      const newOrder: Order = {
        id: this.generateOrderId(),
        price: newPrice,
        quantity: newQuantity,
        total: 0,
        timestamp: Date.now(),
        side: side,
        userId: this.generateUserId()
      };
      this.addOrderToBook(newOrder);
    }
  };


  matchOrders = (newOrder: Order): Trade | null => {
    const { orderBook, testParams } = this.state;
    const oppositeSide = newOrder.side === 'bid' ? orderBook.asks : orderBook.bids;

    if (oppositeSide.length === 0) return null;

    let remainingQuantity = newOrder.quantity;
    let totalValue = 0;
    let totalQuantity = 0;
    const matchedOrders: Order[] = [];

    for (let i = 0; i < oppositeSide.length && remainingQuantity > 0; i++) {
      const oppositeOrder = oppositeSide[i];


      if (!testParams.allowSelfTrade && newOrder.userId === oppositeOrder.userId) {
        continue;
      }


      if ((newOrder.side === 'bid' && newOrder.price >= oppositeOrder.price) ||
        (newOrder.side === 'ask' && newOrder.price <= oppositeOrder.price)) {

        const tradeQuantity = Math.min(remainingQuantity, oppositeOrder.quantity);
        const tradePrice = oppositeOrder.price;

        totalValue += tradeQuantity * tradePrice;
        totalQuantity += tradeQuantity;
        remainingQuantity -= tradeQuantity;

        matchedOrders.push(oppositeOrder);


        if (tradeQuantity === oppositeOrder.quantity) {
          this.removeOrderFromBook(oppositeOrder.id, newOrder.side === 'bid' ? 'ask' : 'bid');
        } else {
          this.modifyOrderInBook(oppositeOrder.id, newOrder.side === 'bid' ? 'ask' : 'bid',
            oppositeOrder.quantity - tradeQuantity);
        }
      }
    }

    if (totalQuantity > 0) {
      const averagePrice = totalValue / totalQuantity;
      const trade: Trade = {
        id: `trade_${++this.tradeIdCounter}`,
        price: parseFloat(averagePrice.toFixed(2)),
        quantity: totalQuantity,
        timestamp: new Date().toLocaleTimeString(),
        side: newOrder.side === 'bid' ? 'buy' : 'sell'
      };

      this.setState(prevState => ({
        trades: [trade, ...prevState.trades.slice(0, 49)],
        currentPrice: parseFloat(averagePrice.toFixed(2)),
        performanceMetrics: {
          ...prevState.performanceMetrics,
          totalTrades: prevState.performanceMetrics.totalTrades + 1
        }
      }));

      return trade;
    }

    return null;
  };


  submitOrder = (order: TestOrder) => {
    const startTime = performance.now();

    try {
      switch (order.type) {
        case 'limit':
          const limitOrder: Order = {
            id: this.generateOrderId(),
            price: order.price!,
            quantity: order.quantity,
            total: 0,
            timestamp: Date.now(),
            side: order.side,
            userId: this.generateUserId()
          };

          const trade = this.matchOrders(limitOrder);
          if (!trade) {
            this.addOrderToBook(limitOrder);
          }
          break;

        case 'market':
          const marketOrder: Order = {
            id: this.generateOrderId(),
            price: order.side === 'bid' ? Number.MAX_SAFE_INTEGER : 0,
            quantity: order.quantity,
            total: 0,
            timestamp: Date.now(),
            side: order.side,
            userId: this.generateUserId()
          };
          this.matchOrders(marketOrder);
          break;

        case 'cancel':
          this.removeOrderFromBook(order.originalOrderId!, order.side);
          break;

        case 'modify':
          this.modifyOrderInBook(order.originalOrderId!, order.side, order.quantity, order.price);
          break;
      }

      const endTime = performance.now();
      const processTime = endTime - startTime;

      this.setState(prevState => ({
        performanceMetrics: {
          ...prevState.performanceMetrics,
          orderProcessTime: parseFloat((processTime).toFixed(3))
        }
      }));

      this.ordersProcessed++;
    } catch (error) {
      var log = this.state.isEnglish ? 'handle order error' : '订单处理错误';
      this.addLog(log);
    }
  };


  runTestScenario = (scenarioName: string) => {
    const scenario = this.state.testScenarios[scenarioName];
    if (!scenario) return;
    var log = this.state.isEnglish ? 'Start running the test scenario' : '开始运行测试场景';
    this.addLog(log + `: ${scenarioName}`);
    scenario.forEach((order, index) => {
      setTimeout(() => {
        this.submitOrder(order);
      }, index * 100);
    });
  };


  runBasicFunctionTest = () => {
    var log = this.state.isEnglish ? 'Start basic functional testing' : '开始基础功能测试';
    this.addLog(log);
    this.runTestScenario('basicFunctionality');
  };


  runMatchingLogicTest = () => {
    var log = this.state.isEnglish ? 'Start matching logic test' : '开始匹配逻辑测试';
    this.addLog(log);
    this.runTestScenario('matchingLogic');
  };


  runSortingRulesTest = () => {
    var log = this.state.isEnglish ? 'Start sorting rule test' : '开始排序规则测试';
    this.addLog(log);
    this.runTestScenario('sortingRules');
  };


  runBoundaryTest = () => {
    var log = this.state.isEnglish ? 'Start boundary condition testing' : '开始边界条件测试';
    this.addLog(log);
    this.runTestScenario('boundaryConditions');
  };


  runPerformanceTest = () => {
    var log = this.state.isEnglish ? 'Start performance testing' : '开始性能测试';
    this.addLog(log);
    this.testStartTime = performance.now();
    this.ordersProcessed = 0;

    this.performanceTestInterval = setInterval(() => {
      const currentTime = performance.now();
      const elapsedTime = (currentTime - this.testStartTime) / 1000;

      if (elapsedTime >= 10) {
        if (this.performanceTestInterval) {
          clearInterval(this.performanceTestInterval);
        }

        const throughput = Math.round(this.ordersProcessed / elapsedTime);
        this.setState(prevState => ({
          performanceMetrics: {
            ...prevState.performanceMetrics,
            throughput: throughput
          }
        }));
        var log2 = this.state.isEnglish ? 'Performance test completed: Throughput ${throughput} orders/second' : '性能测试完成: 吞吐量 ${throughput} 订单/秒';
        this.addLog(log2);
        return;
      }


      for (let i = 0; i < 10; i++) {
        const side = Math.random() > 0.5 ? 'bid' : 'ask';
        const type = Math.random() > 0.7 ? 'market' : 'limit';
        const order: TestOrder = {
          id: this.generateOrderId(),
          type: type,
          side: side,
          price: type === 'limit' ?
            this.state.currentPrice * (0.95 + Math.random() * 0.1) :
            undefined,
          quantity: Math.floor(50 + Math.random() * 200)
        };
        this.submitOrder(order);
      }
    }, 10);
  };


  runConsistencyTest = () => {

    var log = this.state.isEnglish ? 'Start data consistency test' : '开始数据一致性测试';
    this.addLog(log);


    const snapshot = JSON.parse(JSON.stringify(this.state.orderBook));


    const testOrders: TestOrder[] = [
      { id: '1', type: 'limit', side: 'bid', price: 495, quantity: 100 },
      { id: '2', type: 'limit', side: 'ask', price: 505, quantity: 100 },
      { id: '3', type: 'market', side: 'bid', quantity: 50 },
      { id: '4', type: 'cancel', side: 'bid', quantity: 0, originalOrderId: '1' }
    ];

    testOrders.forEach((order, index) => {
      setTimeout(() => {
        this.submitOrder(order);
      }, index * 200);
    });


    setTimeout(() => {
    var log = this.state.isEnglish ? 'Data consistency test completed' : '数据一致性测试完成';

      this.addLog(log);
    }, 1000);
  };


  runThroughputTest = () => {
    var log = this.state.isEnglish ? 'Start high throughput testing' : '开始高吞吐量测试';

    this.addLog(log);

    for (let i = 0; i < 1000; i++) {
      setTimeout(() => {
        const side = Math.random() > 0.5 ? 'bid' : 'ask';
        const order: TestOrder = {
          id: this.generateOrderId(),
          type: 'limit',
          side: side,
          price: this.state.currentPrice * (0.98 + Math.random() * 0.04),
          quantity: Math.floor(10 + Math.random() * 100)
        };
        this.submitOrder(order);
      }, i * 1);
    }
  };


  generateRandomOrders = () => {
    const { orderCount, priceRange, quantityRange, marketOrderRatio, largeOrderRatio } = this.state.testParams;

    for (let i = 0; i < orderCount; i++) {
      const isMarketOrder = Math.random() < marketOrderRatio;
      const isLargeOrder = Math.random() < largeOrderRatio;
      const side = Math.random() > 0.5 ? 'bid' : 'ask';

      let price;
      if (isMarketOrder) {
        price = side === 'bid' ? Number.MAX_SAFE_INTEGER : 0;
      } else {
        price = priceRange.min + Math.random() * (priceRange.max - priceRange.min);
      }

      let quantity = quantityRange.min + Math.random() * (quantityRange.max - quantityRange.min);
      if (isLargeOrder) {
        quantity *= 5;
      }

      const order: TestOrder = {
        id: this.generateOrderId(),
        type: isMarketOrder ? 'market' : 'limit',
        side: side,
        price: isMarketOrder ? undefined : parseFloat(price.toFixed(2)),
        quantity: Math.floor(quantity)
      };

      this.submitOrder(order);
    }

    this.addLog(`${this.state.isEnglish ? 'Created' : '生成了'} ${orderCount} ${this.state.isEnglish ? 'random orders' : '个随机订单'}`);
  };


  toggleTest = () => {
    if (this.state.isTesting) {
      this.stopTest();
    } else {
      this.startTest();
    }
  };

  startTest = () => {
    this.setState({ isTesting: true });
    var log = this.state.isEnglish ? 'start order book test' : '开始订单簿测试';
    this.addLog(log);
    this.testInterval = setInterval(() => {
      this.generateRandomOrders();
    }, this.state.testParams.updateFrequency);
  };

  stopTest = () => {
    this.setState({ isTesting: false });
    var log = this.state.isEnglish ? 'stop order book test' : '停止订单簿测试';
    this.addLog(log);
    if (this.testInterval) {
      clearInterval(this.testInterval);
    }
    if (this.performanceTestInterval) {
      clearInterval(this.performanceTestInterval);
    }
  };


  clearOrderBook = () => {
    this.setState({
      orderBook: {
        bids: [],
        asks: []
      },
      testLogs: [],
      trades: [],
      performanceMetrics: {
        ...this.state.performanceMetrics,
        totalOrders: 0,
        totalTrades: 0
      }
    });
    var log = this.state.isEnglish ? 'order book cleaned' : '订单簿已清空';
    this.addLog(log);
  };


  addLog = (message: string) => {
    const timestamp = new Date().toLocaleTimeString();
    this.setState(prevState => ({
      testLogs: [...prevState.testLogs.slice(-19), `${timestamp} - ${message}`]
    }));
  };


  handleParamChange = (param: keyof TestParams, value: number | boolean) => {
    this.setState(prevState => ({
      testParams: {
        ...prevState.testParams,
        [param]: value
      }
    }));
  };


  handleNestedParamChange = (parent: keyof TestParams, param: keyof Range, value: string) => {
    const numValue = parseFloat(value);
    if (isNaN(numValue)) return;

    this.setState(prevState => ({
      testParams: {
        ...prevState.testParams,
        [parent]: {
          ...prevState.testParams[parent] as Range,
          [param]: numValue
        }
      }
    }));
  };


  handleTabChange = (tab: string) => {
    this.setState({ activeTab: tab });
  };


  toggleTheme = () => {
    this.setState(prevState => ({ isDarkTheme: !prevState.isDarkTheme }));
  };


  toggleLanguage = () => {
    this.setState(prevState => ({ isEnglish: !prevState.isEnglish }));
  };

  componentWillUnmount() {
    if (this.testInterval) {
      clearInterval(this.testInterval);
    }
    if (this.performanceTestInterval) {
      clearInterval(this.performanceTestInterval);
    }
  }

  render() {
    const {
      orderBook, testParams, isTesting, testLogs, currentPrice, trades,
      activeTab, performanceMetrics, isDarkTheme, isEnglish
    } = this.state;

    const maxBidTotal = orderBook.bids[orderBook.bids.length - 1]?.total || 1;
    const maxAskTotal = orderBook.asks[orderBook.asks.length - 1]?.total || 1;


    const texts = {
      title: isEnglish ? "Order Book Test Engine" : "订单簿测试引擎",
      basicTest: isEnglish ? "Basic Test" : "基础测试",
      matchingTest: isEnglish ? "Matching Test" : "匹配测试",
      sortingTest: isEnglish ? "Sorting Test" : "排序测试",
      boundaryTest: isEnglish ? "Boundary Test" : "边界测试",
      performanceTest: isEnglish ? "Performance Test" : "性能测试",
      consistencyTest: isEnglish ? "Consistency Test" : "一致性测试",
      throughputTest: isEnglish ? "Throughput Test" : "吞吐量测试"
    };

    return (
      <div className={`order-book-test ${isDarkTheme ? 'dark-theme' : 'light-theme'}`}>

        <div className="control-header">
          <div className="header-left">
            <h1>{texts.title}</h1>
          </div>
          <div className="header-right">
            <button className="theme-toggle" onClick={this.toggleTheme}>
              {isDarkTheme ? '☀️' : '🌙'}
            </button>
            <button className="language-toggle" onClick={this.toggleLanguage}>
              {isEnglish ? '中' : 'EN'}
            </button>
          </div>
        </div>


        <div className="top-section">

          <div className="horizontal-depth-chart">
            <div className="chart-header">
              <h3>{isEnglish ? "Horizontal Depth Chart" : "水平深度图"}</h3>
              <div className="current-price">{isEnglish ? "Current Price" : "当前价格"}: ${currentPrice}</div>
            </div>
            <div className="depth-chart-content" style={{ padding: '0px', paddingLeft: '10px' }}>
              <div className="chart-container">
                <div className="asks-chart">
                  <div className="chart-title">{isEnglish ? "Asks" : "卖盘"} - {isEnglish ? "Low to High" : "从低到高"}</div>
                  <div className="bars-container" ref={this.asksBarsContainerRef}>

                    {orderBook.asks.map((order, index) => (
                      <HorizontalDepthBar
                        key={`ask-${order.id}`}
                        order={order}
                        type="ask"
                        maxTotal={maxAskTotal}
                        isTop={index === orderBook.asks.length - 1}
                      />
                    ))}
                    {orderBook.asks.length === 0 && (
                      <div className="no-trades" style={{ padding: '20px', textAlign: 'center', color: '#999' }}>
                        {isEnglish ? "No ask orders" : "暂无卖单"}
                      </div>
                    )}
                  </div>
                </div>

                <div className="price-divider">
                  <div className="price-line"></div>
                  <div className="current-price-label">${currentPrice}</div>
                </div>

                <div className="bids-chart">
                  <div className="chart-title">{isEnglish ? "Bids" : "买盘"} - {isEnglish ? "High to Low" : "从高到低"}</div>
                  <div className="bars-container">

                    {orderBook.bids.map((order, index) => (
                      <HorizontalDepthBar
                        key={`bid-${order.id}`}
                        order={order}
                        type="bid"
                        maxTotal={maxBidTotal}
                        isTop={index === 0}
                      />
                    ))}
                  </div>
                </div>
              </div>
            </div>
          </div>


          <div className="ladder-depth-chart">
            <div className="chart-header">
              <h3>{isEnglish ? "Ladder Depth Chart" : "阶梯深度图"}</h3>
              <div className="current-price">{isEnglish ? "Current Price" : "当前价格"}: ${currentPrice}</div>
            </div>
            <div className="depth-chart-content" style={{ padding: '0px' }}>
              <div className="ladder-container">
                <div className="ladder-header">
                  <span>{isEnglish ? "Price" : "价格"}</span>
                  <span>{isEnglish ? "Quantity" : "数量"}</span>
                  <span>{isEnglish ? "Total" : "累计"}</span>
                </div>

                <div className="ladder-asks" ref={this.ladderAsksContainerRef}>

                  {orderBook.asks.map((order, index) => (
                    <LadderRow
                      key={`ask-${order.id}`}
                      order={order}
                      type="ask"
                      isTop={index === orderBook.asks.length - 1}
                    />
                  ))}
                </div>

                <div className="ladder-middle">
                  <div className="middle-price">${currentPrice}</div>
                </div>

                <div className="ladder-bids">

                  {orderBook.bids.map((order, index) => (
                    <LadderRow
                      key={`bid-${order.id}`}
                      order={order}
                      type="bid"
                      isTop={index === 0}
                    />
                  ))}
                </div>
              </div>
            </div>
          </div>


          <div className="trade-log-section">
            <div className="chart-header">
              <h3>{isEnglish ? "Recent Trades" : "最新成交"}</h3>
              <div className="trade-count">{isEnglish ? "Total" : "总数"}: {trades.length}</div>
            </div>
            <div className="trade-log-content" style={{ padding: '0px' }}>
              <div className="trade-log-header">
                <span>{isEnglish ? "Time" : "时间"}</span>
                <span>{isEnglish ? "Side" : "方向"}</span>
                <span>{isEnglish ? "Price" : "价格"}</span>
                <span>{isEnglish ? "Quantity" : "数量"}</span>
              </div>
              <div className="trade-list">
                {trades.map((trade) => (
                  <TradeRow
                    key={trade.id}
                    trade={trade}
                    isEnglish={isEnglish}
                  />
                ))}
                {trades.length === 0 && (
                  <div className="no-trades">
                    {isEnglish ? "No trades yet" : "暂无成交记录"}
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>


        <div className="test-section">
          <div className="tab-header">
            <button
              className={`tab-button ${activeTab === 'basic' ? 'active' : ''}`}
              onClick={() => this.handleTabChange('basic')}
            >
              {texts.basicTest}
            </button>
            <button
              className={`tab-button ${activeTab === 'matching' ? 'active' : ''}`}
              onClick={() => this.handleTabChange('matching')}
            >
              {texts.matchingTest}
            </button>
            <button
              className={`tab-button ${activeTab === 'sorting' ? 'active' : ''}`}
              onClick={() => this.handleTabChange('sorting')}
            >
              {texts.sortingTest}
            </button>
            <button
              className={`tab-button ${activeTab === 'boundary' ? 'active' : ''}`}
              onClick={() => this.handleTabChange('boundary')}
            >
              {texts.boundaryTest}
            </button>
            <button
              className={`tab-button ${activeTab === 'performance' ? 'active' : ''}`}
              onClick={() => this.handleTabChange('performance')}
            >
              {texts.performanceTest}
            </button>
            <button
              className={`tab-button ${activeTab === 'consistency' ? 'active' : ''}`}
              onClick={() => this.handleTabChange('consistency')}
            >
              {texts.consistencyTest}
            </button>
          </div>

          <div className="tab-content">

            {activeTab === 'basic' && (
              <div className="basic-test">
                <h3>{isEnglish ? "Basic Functionality Test" : "基础功能测试"}</h3>
                <div className="test-description">
                  <p>{isEnglish
                    ? "Test basic order operations: add, cancel, modify orders"
                    : "测试基础订单操作：新增、取消、修改订单"}</p>
                </div>

                <div className="param-controls">
                  <div className="param-group">
                    <label>{isEnglish ? "Order Count" : "订单数量"}:</label>
                    <input
                      type="number"
                      value={testParams.orderCount}
                      onChange={(e) => this.handleParamChange('orderCount', parseInt(e.target.value))}
                      disabled={isTesting}
                    />
                  </div>

                  <div className="param-group">
                    <label>{isEnglish ? "Price Range" : "价格范围"}:</label>
                    <div className="range-inputs">
                      <input
                        type="number"
                        value={testParams.priceRange.min}
                        onChange={(e) => this.handleNestedParamChange('priceRange', 'min', e.target.value)}
                        disabled={isTesting}
                      />
                      <span> - </span>
                      <input
                        type="number"
                        value={testParams.priceRange.max}
                        onChange={(e) => this.handleNestedParamChange('priceRange', 'max', e.target.value)}
                        disabled={isTesting}
                      />
                    </div>
                  </div>

                  <div className="param-group">
                    <label>{isEnglish ? "Quantity Range" : "数量范围"}:</label>
                    <div className="range-inputs">
                      <input
                        type="number"
                        value={testParams.quantityRange.min}
                        onChange={(e) => this.handleNestedParamChange('quantityRange', 'min', e.target.value)}
                        disabled={isTesting}
                      />
                      <span> - </span>
                      <input
                        type="number"
                        value={testParams.quantityRange.max}
                        onChange={(e) => this.handleNestedParamChange('quantityRange', 'max', e.target.value)}
                        disabled={isTesting}
                      />
                    </div>
                  </div>

                  <div className="param-group">
                    <label>{isEnglish ? "Update Frequency (ms)" : "更新频率 (ms)"}:</label>
                    <input
                      type="number"
                      value={testParams.updateFrequency}
                      onChange={(e) => this.handleParamChange('updateFrequency', parseInt(e.target.value))}
                      disabled={isTesting}
                    />
                  </div>

                  <div className="param-group">
                    <label>{isEnglish ? "Volatility" : "波动率"}:</label>
                    <input
                      type="number"
                      step="0.01"
                      value={testParams.volatility}
                      onChange={(e) => this.handleParamChange('volatility', parseFloat(e.target.value))}
                      disabled={isTesting}
                    />
                  </div>

                  <div className="param-group">
                    <label>{isEnglish ? "Market Order Ratio" : "市价单比例"}:</label>
                    <input
                      type="number"
                      step="0.01"
                      min="0"
                      max="1"
                      value={testParams.marketOrderRatio}
                      onChange={(e) => this.handleParamChange('marketOrderRatio', parseFloat(e.target.value))}
                      disabled={isTesting}
                    />
                  </div>

                  <div className="param-group">
                    <label>{isEnglish ? "Large Order Ratio" : "大单比例"}:</label>
                    <input
                      type="number"
                      step="0.01"
                      min="0"
                      max="1"
                      value={testParams.largeOrderRatio}
                      onChange={(e) => this.handleParamChange('largeOrderRatio', parseFloat(e.target.value))}
                      disabled={isTesting}
                    />
                  </div>

                  <div className="param-group">
                    <label>{isEnglish ? "Cancel Order Ratio" : "撤单比例"}:</label>
                    <input
                      type="number"
                      step="0.01"
                      min="0"
                      max="1"
                      value={testParams.cancelOrderRatio}
                      onChange={(e) => this.handleParamChange('cancelOrderRatio', parseFloat(e.target.value))}
                      disabled={isTesting}
                    />
                  </div>

                  <div className="param-group">
                    <label>{isEnglish ? "Test User Count" : "测试用户数量"}:</label>
                    <input
                      type="number"
                      value={testParams.testUserCount}
                      onChange={(e) => this.handleParamChange('testUserCount', parseInt(e.target.value))}
                      disabled={isTesting}
                    />
                  </div>

                  <div className="param-group checkbox-group">
                    <label>
                      <input
                        type="checkbox"
                        checked={testParams.allowSelfTrade}
                        onChange={(e) => this.handleParamChange('allowSelfTrade', e.target.checked)}
                        disabled={isTesting}
                      />
                      {isEnglish ? "Allow Self Trading" : "允许自成交"}
                    </label>
                  </div>
                </div>

                <div className="action-buttons">
                  <button
                    className={`toggle-btn ${isTesting ? 'stop' : 'start'}`}
                    onClick={this.toggleTest}
                  >
                    {isTesting
                      ? (isEnglish ? "Stop Test" : "停止测试")
                      : (isEnglish ? "Start Test" : "开始测试")}
                  </button>

                  <button
                    className="generate-btn"
                    onClick={this.generateRandomOrders}
                    disabled={isTesting}
                  >
                    {isEnglish ? "Generate Orders" : "生成随机订单"}
                  </button>

                  <button
                    className="clear-btn"
                    onClick={this.clearOrderBook}
                    disabled={isTesting}
                  >
                    {isEnglish ? "Clear Order Book" : "清空订单簿"}
                  </button>

                  <button
                    className="test-scenario-btn"
                    onClick={this.runBasicFunctionTest}
                    disabled={isTesting}
                  >
                    {isEnglish ? "Run Basic Test" : "运行基础测试"}
                  </button>
                </div>
              </div>
            )}


            {activeTab === 'matching' && (
              <div className="matching-test">
                <h3>{isEnglish ? "Matching Logic Test" : "核心匹配逻辑测试"}</h3>
                <div className="test-description">
                  <p>{isEnglish
                    ? "Test market orders, limit orders, partial fills, and trade price validation"
                    : "测试市价单、限价单、部分成交和成交价格验证"}</p>
                </div>
                <div className="test-actions">
                  <button
                    className="test-scenario-btn"
                    onClick={this.runMatchingLogicTest}
                    disabled={isTesting}
                  >
                    {isEnglish ? "Run Matching Test" : "运行匹配测试"}
                  </button>
                  <button
                    className="test-scenario-btn"
                    onClick={this.runThroughputTest}
                    disabled={isTesting}
                  >
                    {isEnglish ? "High Throughput Test" : "高吞吐量测试"}
                  </button>
                </div>
              </div>
            )}


            {activeTab === 'sorting' && (
              <div className="sorting-test">
                <h3>{isEnglish ? "Sorting Rules Test" : "排序规则测试"}</h3>
                <div className="test-description">
                  <p>{isEnglish
                    ? "Test price-time priority: better prices first, then earlier times"
                    : "测试价格时间优先原则：价格优先，时间优先"}</p>
                </div>
                <div className="test-actions">
                  <button
                    className="test-scenario-btn"
                    onClick={this.runSortingRulesTest}
                    disabled={isTesting}
                  >
                    {isEnglish ? "Run Sorting Test" : "运行排序测试"}
                  </button>
                </div>
              </div>
            )}


            {activeTab === 'boundary' && (
              <div className="boundary-test">
                <h3>{isEnglish ? "Boundary Conditions Test" : "边界和异常测试"}</h3>
                <div className="test-description">
                  <p>{isEnglish
                    ? "Test invalid inputs, duplicate operations, and extreme values"
                    : "测试无效输入、重复操作和极端值处理"}</p>
                </div>
                <div className="test-actions">
                  <button
                    className="test-scenario-btn"
                    onClick={this.runBoundaryTest}
                    disabled={isTesting}
                  >
                    {isEnglish ? "Run Boundary Test" : "运行边界测试"}
                  </button>
                </div>
              </div>
            )}


            {activeTab === 'performance' && (
              <div className="performance-test">
                <h3>{isEnglish ? "Performance Metrics" : "性能指标监控"}</h3>
                <div className="performance-metrics">
                  <div className="metric-card">
                    <div className="metric-value">{performanceMetrics.orderProcessTime}ms</div>
                    <div className="metric-label">{isEnglish ? "Avg Process Time" : "平均处理时间"}</div>
                  </div>
                  <div className="metric-card">
                    <div className="metric-value">{performanceMetrics.throughput}</div>
                    <div className="metric-label">{isEnglish ? "Throughput (orders/s)" : "吞吐量 (订单/秒)"}</div>
                  </div>
                  <div className="metric-card">
                    <div className="metric-value">{performanceMetrics.matchEfficiency}%</div>
                    <div className="metric-label">{isEnglish ? "Match Efficiency" : "匹配效率"}</div>
                  </div>
                  <div className="metric-card">
                    <div className="metric-value">{performanceMetrics.memoryUsage}MB</div>
                    <div className="metric-label">{isEnglish ? "Memory Usage" : "内存使用"}</div>
                  </div>
                  <div className="metric-card">
                    <div className="metric-value">{performanceMetrics.totalOrders}</div>
                    <div className="metric-label">{isEnglish ? "Total Orders" : "总订单数"}</div>
                  </div>
                  <div className="metric-card">
                    <div className="metric-value">{performanceMetrics.totalTrades}</div>
                    <div className="metric-label">{isEnglish ? "Total Trades" : "总成交数"}</div>
                  </div>
                </div>
                <div className="performance-actions">
                  <button
                    className="test-scenario-btn"
                    onClick={this.runPerformanceTest}
                    disabled={isTesting}
                  >
                    {isEnglish ? "Run Performance Test" : "运行性能测试"}
                  </button>
                </div>
              </div>
            )}


            {activeTab === 'consistency' && (
              <div className="consistency-test">
                <h3>{isEnglish ? "Data Consistency Test" : "数据一致性测试"}</h3>
                <div className="test-description">
                  <p>{isEnglish
                    ? "Verify event stream consistency and snapshot accuracy"
                    : "验证事件流一致性和快照准确性"}</p>
                </div>
                <div className="test-actions">
                  <button
                    className="test-scenario-btn"
                    onClick={this.runConsistencyTest}
                    disabled={isTesting}
                  >
                    {isEnglish ? "Run Consistency Test" : "运行一致性测试"}
                  </button>
                </div>
              </div>
            )}


            <div className="test-logs">
              <h4>{isEnglish ? "Test Logs" : "测试日志"}</h4>
              <div className="log-content">
                {testLogs.map((log, index) => (
                  <div key={index} className="log-entry">{log}</div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div >
    );
  }
}


class HorizontalDepthBar extends Component<{
  order: Order;
  type: 'bid' | 'ask';
  maxTotal: number;
  isTop: boolean;
}> {
  render() {
    const { order, type, maxTotal, isTop } = this.props;
    const progressWidth = (order.total / maxTotal) * 100;

    return (
      <div className={`depth-bar ${type} ${isTop ? 'top-level' : ''}`}>
        <div className="bar-info">
          <span className="price">${order.price.toFixed(2)}</span>
          <span className="quantity">{order.quantity}</span>
        </div>
        <div className="bar-container">
          <div
            className={`bar-fill ${type}`}
            style={{ width: `${progressWidth}%` }}
          ></div>
        </div>
      </div>
    );
  }
}


class LadderRow extends Component<{
  order: Order;
  type: 'bid' | 'ask';
  isTop: boolean;
}> {
  render() {
    const { order, type, isTop } = this.props;

    return (
      <div className={`ladder-row ${type} ${isTop ? 'top-level' : ''}`}>
        <span className="ladder-price">${order.price.toFixed(2)}</span>
        <span className="ladder-quantity">{order.quantity}</span>
        <span className="ladder-total">{order.total}</span>
      </div>
    );
  }
}


class TradeRow extends Component<{
  trade: Trade;
  isEnglish: boolean;
}> {
  render() {
    const { trade, isEnglish } = this.props;

    return (
      <div className={`trade-row ${trade.side}`}>
        <span className="trade-time">{trade.timestamp}</span>
        <span className={`trade-side ${trade.side}`}>
          {trade.side === 'buy'
            ? (isEnglish ? 'BUY' : '买入')
            : (isEnglish ? 'SELL' : '卖出')}
        </span>
        <span className="trade-price">${trade.price.toFixed(2)}</span>
        <span className="trade-quantity">{trade.quantity}</span>
      </div>
    );
  }
}

export default OrderBookTest;