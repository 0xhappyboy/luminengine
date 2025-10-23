<h1 align="center">
    ✨ Luminengine
</h1>
<h4 align="center">
高性能金融量化交易订单簿撮合框架.
</h4>
<p align="center">
  <a href="https://github.com/0xhappyboy/arbitrage/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-GPL3.0-d1d1f6.svg?style=flat&labelColor=1C2C2E&color=BEC5C9&logo=googledocs&label=license&logoColor=BEC5C9" alt="License"></a>
</p>
<p align="center">
<a href="./README_zh-CN.md">简体中文</a> | <a href="./README.md">English</a>
</p>

## 支持订单类型

<div class="markdown-accessiblity-table" style="display: block; width: 100% !important; max-width: none !important;">
<table width="100%">
  <tr>
    <th width="50%">Order Type</th>
    <th width="50%">Status</th>
  </tr>
  <tr>
    <td>Limit Order</td>
    <td>✅</td>
  </tr>
   <tr>
    <td>Market Order</td>
    <td>✅</td>
  </tr>
</table>
</div>

## 订单类型

### 市价单

#### 买方向的市价单

```math
\begin{aligned}
&\text{Let } M_{\text{buy}} = \text{total market buy orders} \\
&\text{Let } P_{\text{ask}} = [p_1, p_2, \dots, p_n] \text{sell ​​order prices in ascending order} \\
&\text{Let } Q_{\text{ask}}(p_i) = \text{ in price } p_i \text{total sell orders} \\
\\
&\text{For } i = 1 \text{ to } n: \\
&\quad \text{If } M_{\text{buy}} \leq 0: \text{ break} \\
&\quad \text{MatchQty} = \min(M_{\text{buy}}, Q_{\text{ask}}(p_i)) \\
&\quad \text{Execute trade at price } p_i \text{ with quantity MatchQty} \\
&\quad M_{\text{buy}} = M_{\text{buy}} - \text{MatchQty} \\
&\quad Q_{\text{ask}}(p_i) = Q_{\text{ask}}(p_i) - \text{MatchQty}
\end{aligned}
```

#### 卖方向的市价单

```math
\begin{aligned}
&\text{Let } M_{\text{sell}} = \text{total market sell orders} \\
&\text{Let } P_{\text{bid}} = [p_1, p_2, \dots, p_n] \text{buy order prices in descending order} \\
&\text{Let } Q_{\text{bid}}(p_i) = \text{in ptice} p_i \text{total buy orders} \\
\\
&\text{For } i = 1 \text{ to } n: \\
&\quad \text{If } M_{\text{sell}} \leq 0: \text{ break} \\
&\quad \text{MatchQty} = \min(M_{\text{sell}}, Q_{\text{bid}}(p_i)) \\
&\quad \text{Execute trade at price } p_i \text{ with quantity MatchQty} \\
&\quad M_{\text{sell}} = M_{\text{sell}} - \text{MatchQty} \\
&\quad Q_{\text{bid}}(p_i) = Q_{\text{bid}}(p_i) - \text{MatchQty}
\end{aligned}
```

#### 平均成交价计算

```math
\text{AvgPrice} = \frac{\sum_{i=1}^{k} (p_i \times q_i)}{\sum_{i=1}^{k} q_i}
```

### 冰山订单

![iceberg order manager](assets/architecture/iceberg_manager_zhCN.png)

## 资源

- <a href="https://github.com/0xhappyboy/luminengine/tree/main/builder/rpc/protos">RPC Proto 文件</a>
